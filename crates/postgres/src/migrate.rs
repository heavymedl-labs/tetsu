//! The migration runner.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use tetsu_kernel::{Migration, Module, Storage};
use tokio_postgres::Client;

use crate::connection::{Error, Settings, connect};

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
/// Fails if the modules break the kernel contract, the database is not UTF8, the lock stays held, a file fails,
/// or an applied file has changed.
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
    check_utf8(&client).await?;
    take_lock(&client, wait).await?;
    // Every module is checked before any file is applied.
    let mut plan = Vec::new();
    for storage in modules.iter().filter_map(|module| module.storage()) {
        plan.push((storage.schema, pending(&client, storage).await?));
    }
    for (schema, migrations) in plan {
        for migration in migrations {
            apply(&client, schema, migration).await?;
        }
    }
    // Closing the connection releases the lock, even after a failure above.
    Ok(())
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
                "schema {schema:?} must be a lowercase identifier of at most 63 bytes, and not a system schema"
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
/// and the runner can quote it safely. `public`, `information_schema`, and the reserved `pg_` prefix are never a
/// module's own.
fn plain_schema(schema: &str) -> bool {
    let mut chars = schema.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && schema.len() <= 63
        && schema != "public"
        && schema != "information_schema"
        && !schema.starts_with("pg_")
}

/// Fails unless the database stores text as UTF8. Modules check text before storing it, and those checks
/// assume the database can hold any Unicode text, so anything that writes checked text calls this first.
///
/// # Errors
/// Fails if the database is unreachable or uses another encoding.
pub async fn require_utf8(settings: &Settings) -> Result<(), Error> {
    check_utf8(&connect(settings).await?).await
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
    client
        .batch_execute(&format!(
            r#"CREATE SCHEMA IF NOT EXISTS "{schema}";
             CREATE TABLE IF NOT EXISTS "{schema}".migrations (
                 version int PRIMARY KEY,
                 sql text NOT NULL,
                 applied_at timestamptz NOT NULL DEFAULT now()
             )"#
        ))
        .await?;
    client.batch_execute(&read_only_record(schema)).await?;
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
/// role is through default privileges, keeps reading it but loses every other right, on each run, so a later grant
/// goes too. It names no role: `PUBLIC` and every role but the owner and the runner lose what the owner granted on
/// the table, with anything granted on from it. Column grants and grants from other roles are left alone.
fn read_only_record(schema: &str) -> String {
    let table = format!(r#""{schema}".migrations"#);
    let rights = "INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER, MAINTAIN";
    format!(
        r"REVOKE {rights} ON {table} FROM PUBLIC CASCADE;
        DO $$
        DECLARE grantee regrole;
        BEGIN
          FOR grantee IN
            SELECT DISTINCT a.grantee::regrole
            FROM pg_catalog.pg_class c, pg_catalog.aclexplode(c.relacl) a
            WHERE c.oid = '{table}'::regclass AND a.privilege_type <> 'SELECT'
              AND a.grantee NOT IN (0, c.relowner)
              AND a.grantee <> (SELECT oid FROM pg_catalog.pg_roles WHERE rolname = current_user)
          LOOP
            EXECUTE format('REVOKE {rights} ON {table} FROM %s CASCADE', grantee);
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

/// Runs one file with its schema first on the search path, so an unqualified name lands there.
async fn apply(client: &Client, schema: &str, migration: &Migration) -> Result<(), Error> {
    client
        .batch_execute(&format!(r#"SET search_path TO "{schema}""#))
        .await?;
    apply_in_schema(client, schema, migration).await
}

async fn apply_in_schema(
    client: &Client,
    schema: &str,
    migration: &Migration,
) -> Result<(), Error> {
    let record = format!(
        r#"INSERT INTO "{schema}".migrations (version, sql) VALUES ({}, $1)"#,
        migration.version
    );
    if let Some(statement) = migration.sql.strip_prefix(NO_TRANSACTION) {
        return apply_outside_transaction(client, schema, migration, statement, &record).await;
    }
    // The change and its record commit together, so a crash cannot leave one without the other.
    client.batch_execute("BEGIN").await?;
    let applied = async {
        client.batch_execute(migration.sql).await?;
        client.batch_execute(RESET_SESSION).await?;
        client.execute(&record, &[&migration.sql]).await
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
    statement: &str,
    record: &str,
) -> Result<(), Error> {
    if statement.trim().trim_end_matches(';').contains(';') {
        return Err(format!(
            "{schema} version {}: a file outside a transaction holds one statement",
            migration.version
        )
        .into());
    }
    client.batch_execute(migration.sql).await?;
    client.batch_execute(RESET_SESSION).await?;
    refuse_invalid_indexes(client, schema).await?;
    client.execute(record, &[&migration.sql]).await?;
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
