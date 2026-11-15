//! The migration runner.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use tetsu_kernel::{Migration, Module, Storage};
use tokio_postgres::Client;

use crate::connection::{Error, Settings, connect};
use crate::roles::{APP_ROLE, MAX_SCHEMA, prepare_module, require_bound_runner, require_role};

/// Taken by polling, never by blocking, which can deadlock with
/// `CREATE INDEX CONCURRENTLY` (ADR-0011).
pub const MIGRATION_LOCK: i64 = 7_243_001;
const LOCK_WAIT: Duration = Duration::from_secs(60);
const LOCK_POLL: Duration = Duration::from_millis(100);
const NO_TRANSACTION: &str = "-- tetsu:no-transaction";
/// Run right after each file, before the runner's own queries, so nothing the file set reaches its record or the
/// next file: its role and session authorization, which `RESET ALL` leaves alone, every setting, such as
/// `client_encoding`, and its temp tables, which come before the schema on the search path.
const RESET_SESSION: &str = "RESET SESSION AUTHORIZATION; RESET ROLE; RESET ALL; DISCARD TEMP";

/// Applies each module's new migrations.
///
/// # Errors
/// Fails if the modules break the kernel contract, the server is older than PostgreSQL 18.7 or the database is not
/// UTF8, the application role is missing, the lock stays held, a file fails, or an applied file has changed.
pub async fn migrate(settings: &Settings, modules: &[&dyn Module]) -> Result<(), Error> {
    migrate_with_wait(settings, modules, LOCK_WAIT).await
}

/// [`migrate`], with a chosen wait for the lock.
///
/// # Errors
/// As [`migrate`].
pub async fn migrate_with_wait(
    settings: &Settings,
    modules: &[&dyn Module],
    wait: Duration,
) -> Result<(), Error> {
    check_contract(modules)?;
    let client = connect(settings).await?;
    check_before_migrating(&client).await?;
    take_lock(&client, wait).await?;
    let plan = plan(&client, modules).await?;
    // Each file ends by taking back rights on every module's record, since one may grant on every table in a
    // schema, its own or another's.
    let lock_down = plan
        .iter()
        .map(|(schema, _)| read_only_record(schema))
        .collect::<Vec<_>>()
        .join(";\n");
    for (schema, migrations) in plan {
        for migration in migrations {
            apply(&client, schema, migration, &lock_down).await?;
        }
    }
    // Closing the connection releases the lock, even after a failure above.
    Ok(())
}

/// Each module's new files. Every module is checked before any file is applied, and its roles are made before any
/// file, so a file can grant to them and hand functions to its lookup role.
async fn plan(
    client: &Client,
    modules: &[&dyn Module],
) -> Result<Vec<(&'static str, Vec<&'static Migration>)>, Error> {
    let mut plan = Vec::new();
    for storage in modules.iter().filter_map(|module| module.storage()) {
        plan.push((storage.schema, pending(client, storage).await?));
    }
    for (schema, _) in &plan {
        prepare_module(client, schema).await?;
    }
    Ok(plan)
}

/// The server, the runner, and the application role are fit to migrate, before the lock is taken.
async fn check_before_migrating(client: &Client) -> Result<(), Error> {
    check_server(client).await?;
    require_bound_runner(client).await?;
    require_role(client, APP_ROLE).await
}

/// Refuses, before touching the database, modules that break the kernel contract: names and schemas are unique,
/// a schema is a lowercase identifier Postgres keeps as written, and versions grow.
fn check_contract(modules: &[&dyn Module]) -> Result<(), Error> {
    let (mut names, mut schemas) = (HashSet::new(), HashSet::new());
    for module in modules {
        if !names.insert(module.name()) {
            return Err(format!("module {} is registered twice", module.name()).into());
        }
        let Some(Storage { schema, migrations }) = module.storage() else {
            continue;
        };
        if !plain_schema(schema) {
            return Err(format!(
                "schema {schema:?} must be a lowercase identifier of at most {MAX_SCHEMA} bytes, not a system or reserved name, and not end in _lookup"
            )
            .into());
        }
        if !schemas.insert(schema) {
            return Err(format!("schema {schema} belongs to two modules").into());
        }
        if migrations
            .windows(2)
            .any(|pair| pair[0].version >= pair[1].version)
        {
            return Err(format!("{schema} versions must grow").into());
        }
    }
    Ok(())
}

/// Lowercase letters, digits, and underscores, not starting with a digit, so a module's own SQL can name it unquoted
/// and the runner can quote it safely. It leaves room for the module's role names (ADR-0042). `public`, `information_schema`, and the reserved `pg_` prefix are never a
/// module's own.
fn plain_schema(schema: &str) -> bool {
    let mut chars = schema.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && schema.len() <= MAX_SCHEMA
        && schema != "public"
        && schema != "information_schema"
        && !schema.starts_with("pg_")
        && !RESERVED_SCHEMAS.contains(&schema)
        && !schema.ends_with("_lookup")
}

/// Schemas whose module role would be another of Tetsu's roles, or the demo's migration role, or one the catalog
/// checks read as a lookup role. A schema ending in `_lookup` would be another module's lookup role.
const RESERVED_SCHEMAS: &[&str] = &["app", "owner", "migrate", "lookup", "delivery"];

/// The oldest server Tetsu runs on: 18.7 fixes CVE-2026-116311 and CVE-2026-116316 in row security and definer
/// functions, after 18.6 fixed CVE-2026-14666 (ADR-0042, ADR-0060).
pub const MIN_SERVER_VERSION: i32 = 180_007;

/// Fails unless the server is PostgreSQL 18.7 or later and the database stores text as UTF8. Modules check text
/// before storing it, and those checks assume the database can hold any Unicode text, so anything that writes
/// checked text calls this first, and so does anything that serves.
///
/// # Errors
/// Fails if the database is unreachable, the server is older, or the database uses another encoding.
pub async fn require_supported_server(settings: &Settings) -> Result<(), Error> {
    check_server(&connect(settings).await?).await
}

/// The test server runs 18.7, so no test reaches the refusal itself; `supported` is tested alone.
async fn check_server(client: &Client) -> Result<(), Error> {
    let row = client
        .query_one(
            "SELECT current_setting('server_version_num')::int AS version",
            &[],
        )
        .await?;
    supported(row.try_get("version")?)?;
    check_utf8(client).await
}

/// Whether a `server_version_num` is new enough.
fn supported(version: i32) -> Result<(), Error> {
    if version >= MIN_SERVER_VERSION {
        Ok(())
    } else {
        Err(format!(
            "PostgreSQL {}.{} is too old; Tetsu needs {}.{} or later",
            version / 10_000,
            version % 10_000,
            MIN_SERVER_VERSION / 10_000,
            MIN_SERVER_VERSION % 10_000
        )
        .into())
    }
}

async fn check_utf8(client: &Client) -> Result<(), Error> {
    let row = client
        .query_one("SELECT current_setting('server_encoding') AS encoding", &[])
        .await?;
    let encoding: String = row.try_get("encoding")?;
    if encoding == "UTF8" {
        Ok(())
    } else {
        Err(format!("the database must use UTF8, not {encoding}").into())
    }
}

async fn take_lock(client: &Client, wait: Duration) -> Result<(), Error> {
    let started = Instant::now();
    loop {
        let row = client
            .query_one(
                "SELECT pg_try_advisory_lock($1) AS taken",
                &[&MIGRATION_LOCK],
            )
            .await?;
        if row.try_get::<_, bool>("taken")? {
            return Ok(());
        }
        if started.elapsed() >= wait {
            return Err("another migration run still holds the lock".into());
        }
        tokio::time::sleep(LOCK_POLL).await;
    }
}

/// Creates the module's schema and tracking table if needed, then returns its new files.
async fn pending(client: &Client, storage: Storage) -> Result<Vec<&'static Migration>, Error> {
    let Storage { schema, migrations } = storage;
    // One batch is one transaction, so no other role can write to a new table before its rights are taken back.
    client
        .batch_execute(&format!(
            r#"CREATE SCHEMA IF NOT EXISTS "{schema}";
             CREATE TABLE IF NOT EXISTS "{schema}".migrations (
                 version int PRIMARY KEY,
                 sql text NOT NULL,
                 applied_at timestamptz NOT NULL DEFAULT now()
             );
             {}"#,
            read_only_record(schema)
        ))
        .await?;
    let applied = client
        .query(
            &format!(r#"SELECT version, sql FROM "{schema}".migrations"#),
            &[],
        )
        .await?
        .iter()
        .map(|row| Ok((row.try_get("version")?, row.try_get("sql")?)))
        .collect::<Result<HashMap<i32, String>, tokio_postgres::Error>>()?;
    new_files(schema, migrations, &applied)
}

/// Only the runner writes a module's record of applied files. A role given rights on every new table, as the app's
/// role is through default privileges, keeps reading it but loses every other right, in the transaction that creates
/// it and after each file, inside its transaction when it has one, so a grant from a file or from outside goes too.
/// It names no role: `PUBLIC` and every role but the owner lose what the owner granted on the table and its columns,
/// with anything granted on from it. So the runner must own the table, which it does when it created it. Grants
/// from other roles, rights through roles such as `pg_write_all_data`, and views over the table are left alone.
fn read_only_record(schema: &str) -> String {
    let table = format!(r#""{schema}".migrations"#);
    let rights = "INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER, MAINTAIN";
    // Every name is qualified, so nothing a role creates on the search path stands in for it.
    format!(
        r"DO $$
        DECLARE grantee text;
        BEGIN
          FOR grantee IN
            SELECT DISTINCT CASE g.grantee WHEN 0 THEN 'PUBLIC' ELSE g.grantee::pg_catalog.regrole::text END
            FROM pg_catalog.pg_class c,
              LATERAL (SELECT a.grantee, a.privilege_type FROM pg_catalog.aclexplode(c.relacl) a
                       UNION ALL
                       SELECT a.grantee, a.privilege_type FROM pg_catalog.pg_attribute t,
                         pg_catalog.aclexplode(t.attacl) a WHERE t.attrelid = c.oid) g
            WHERE c.oid = '{table}'::pg_catalog.regclass AND g.privilege_type <> 'SELECT'
              AND g.grantee <> c.relowner
          LOOP
            EXECUTE pg_catalog.format('REVOKE {rights} ON {table} FROM %s CASCADE', grantee);
          END LOOP;
        END $$"
    )
}

/// The files not yet applied. Fails if an applied file changed or is missing from the code, or a new file comes
/// before an applied one.
fn new_files<'a>(
    schema: &str,
    migrations: &'a [Migration],
    applied: &HashMap<i32, String>,
) -> Result<Vec<&'a Migration>, Error> {
    if let Some(changed) = migrations
        .iter()
        .find(|m| applied.get(&m.version).is_some_and(|sql| sql != m.sql))
    {
        return Err(format!(
            "{schema} version {} changed after it was applied",
            changed.version
        )
        .into());
    }
    if let Some(missing) = applied
        .keys()
        .filter(|version| !migrations.iter().any(|m| m.version == **version))
        .min()
    {
        return Err(
            format!("{schema} version {missing} is applied but missing from the code").into(),
        );
    }
    let is_new = |migration: &&Migration| !applied.contains_key(&migration.version);
    let new: Vec<_> = migrations.iter().skip_while(|m| !is_new(m)).collect();
    if let (Some(first), Some(later)) = (new.first(), new.iter().find(|m| !is_new(m))) {
        return Err(format!(
            "{schema} version {} is below {}, which is applied",
            first.version, later.version
        )
        .into());
    }
    Ok(new)
}

/// Runs one file with its schema first on the search path, so an unqualified name lands there, then `lock_down`.
async fn apply(
    client: &Client,
    schema: &str,
    migration: &Migration,
    lock_down: &str,
) -> Result<(), Error> {
    client
        .batch_execute(&format!(r#"SET search_path TO "{schema}""#))
        .await?;
    apply_in_schema(client, schema, migration, lock_down).await
}

fn record(schema: &str, migration: &Migration) -> String {
    format!(
        r#"INSERT INTO "{schema}".migrations (version, sql) VALUES ({}, $1)"#,
        migration.version
    )
}

async fn apply_in_schema(
    client: &Client,
    schema: &str,
    migration: &Migration,
    lock_down: &str,
) -> Result<(), Error> {
    if migration.sql.starts_with(NO_TRANSACTION) {
        return apply_outside_transaction(client, schema, migration, lock_down).await;
    }
    // The change, its record, and taking back what it granted commit together, so a crash cannot leave one without
    // the others.
    client.batch_execute("BEGIN").await?;
    let applied = async {
        client.batch_execute(migration.sql).await?;
        client.batch_execute(RESET_SESSION).await?;
        client.batch_execute(lock_down).await?;
        client
            .execute(&record(schema, migration), &[&migration.sql])
            .await
    };
    if let Err(error) = applied.await {
        client.batch_execute("ROLLBACK").await?;
        return Err(error.into());
    }
    client.batch_execute("COMMIT").await?;
    Ok(())
}

/// For statements Postgres refuses to run in a transaction, such as `CREATE INDEX CONCURRENTLY`.
async fn apply_outside_transaction(
    client: &Client,
    schema: &str,
    migration: &Migration,
    lock_down: &str,
) -> Result<(), Error> {
    let statement = &migration.sql[NO_TRANSACTION.len()..];
    if statement.trim().trim_end_matches(';').contains(';') {
        return Err(format!(
            "{schema} version {}: a file outside a transaction holds one statement",
            migration.version
        )
        .into());
    }
    client.batch_execute(migration.sql).await?;
    client.batch_execute(RESET_SESSION).await?;
    client.batch_execute(lock_down).await?;
    refuse_invalid_indexes(client, schema).await?;
    client
        .execute(&record(schema, migration), &[&migration.sql])
        .await?;
    Ok(())
}

/// A failed `CREATE INDEX CONCURRENTLY` leaves an invalid index behind.
async fn refuse_invalid_indexes(client: &Client, schema: &str) -> Result<(), Error> {
    let invalid = client
        .query(
            "SELECT i.indexrelid::pg_catalog.regclass::text AS name FROM pg_catalog.pg_index i
             JOIN pg_catalog.pg_class c ON c.oid = i.indexrelid
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND NOT i.indisvalid",
            &[&schema],
        )
        .await?;
    match invalid.first() {
        Some(row) => Err(format!(
            "invalid index {} needs dropping before a rerun",
            row.try_get::<_, String>("name")?
        )
        .into()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The floor rises with each PostgreSQL advisory on row security, roles, or definer functions (ADR-0060).
    #[test]
    fn the_server_must_be_18_7_or_later() {
        for old in [170_009, 180_000, 180_006] {
            let error = supported(old).unwrap_err().to_string();
            assert!(error.contains("needs 18.7 or later"), "{error}");
        }
        assert_eq!(
            supported(180_006).unwrap_err().to_string(),
            "PostgreSQL 18.6 is too old; Tetsu needs 18.7 or later"
        );
        for new in [180_007, 180_010, 190_000] {
            supported(new).unwrap();
        }
    }
}
