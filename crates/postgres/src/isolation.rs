//! The catalog checks ADR-0042 asks for: every way the modules' tables, policies, and roles break its rules.

use tokio_postgres::Client;

use crate::connection::{Error, Settings, connect};
use crate::migrate::require_supported_server;
use crate::roles::{APP_ROLE, quoted};

const VIOLATIONS: &str = include_str!("../sql/isolation_violations.sql");

/// Each problem the catalog shows in these schemas, in a stable order; none means the rules hold. Run it as any
/// role that can read the catalog, after migrating.
///
/// # Errors
/// Fails if the query fails.
pub async fn isolation_violations(
    client: &mut Client,
    schemas: &[&str],
) -> Result<Vec<String>, Error> {
    isolation_violations_for(client, schemas, APP_ROLE).await
}

/// [`isolation_violations`] for another application role, so a test can break the rules without touching the
/// role every test database shares.
///
/// # Errors
/// Fails if the query fails.
#[doc(hidden)]
pub async fn isolation_violations_for(
    client: &mut Client,
    schemas: &[&str],
    app: &str,
) -> Result<Vec<String>, Error> {
    let schemas: Vec<String> = schemas.iter().map(ToString::to_string).collect();
    // In a transaction of its own, whose search path is the catalog alone, so no object a module made can stand in
    // for a function the checks call or print. Taking the client mutably means no caller's transaction is open, and
    // dropping it rolls back.
    let transaction = client.transaction().await?;
    transaction
        .query_one(
            "SELECT pg_catalog.set_config('search_path', 'pg_catalog', true)",
            &[],
        )
        .await?;
    let rows = transaction.query(VIOLATIONS, &[&schemas, &app]).await?;
    transaction.rollback().await?;
    Ok(rows
        .iter()
        .map(|row| row.try_get("problem"))
        .collect::<Result<_, _>>()?)
}

/// Every module's schema in the database, found by the record of applied files the runner keeps in each, with the
/// runner's own columns, so another tool's table of the same name is not taken for one.
///
/// # Errors
/// Fails if the query fails.
pub async fn module_schemas(client: &Client) -> Result<Vec<String>, Error> {
    let rows = client
        .query(
            "SELECT n.nspname FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE c.relname = 'migrations' AND c.relkind = 'r' AND n.nspname <> 'public'
               AND (SELECT pg_catalog.array_agg(a.attname::text ORDER BY a.attnum) FROM pg_catalog.pg_attribute a
                    WHERE a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped)
                   = ARRAY['version', 'sql', 'applied_at']
             ORDER BY 1",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| row.try_get(0))
        .collect::<Result<_, _>>()?)
}

/// Each tenant-owned table in these schemas that shows its module's role any row while no tenant is set, which
/// row-level security forbids (ADR-0042). Every count runs in a transaction that is rolled back. A partition is
/// counted through its parent, since the module role holds no rights on it.
///
/// # Errors
/// Fails if a query fails, such as when the connection's role may not act as a module's.
pub async fn rows_seen_without_a_tenant(
    client: &mut Client,
    schemas: &[&str],
) -> Result<Vec<String>, Error> {
    let schemas: Vec<String> = schemas.iter().map(ToString::to_string).collect();
    let tables = client
        .query(
            "SELECT n.nspname, c.relname FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = any($1) AND c.relkind IN ('r', 'p') AND NOT c.relispartition
               AND pg_catalog.obj_description(c.oid, 'pg_class') = 'tetsu:tenant-owned'
             ORDER BY 1, 2",
            &[&schemas],
        )
        .await?;
    let mut seen = Vec::new();
    for table in &tables {
        let (schema, name): (String, String) = (table.try_get(0)?, table.try_get(1)?);
        let rows = count_as_module(client, &schema, &name).await?;
        if rows > 0 {
            seen.push(format!(
                "{schema}.{name} shows {rows} rows with no tenant set"
            ));
        }
    }
    Ok(seen)
}

/// The rows a table shows its module's role, in a transaction that is always rolled back.
async fn count_as_module(client: &mut Client, schema: &str, name: &str) -> Result<i64, Error> {
    let transaction = client.transaction().await?;
    transaction
        .query_one(
            "SELECT pg_catalog.set_config('role', $1, true),
                    pg_catalog.set_config('app.tenant_id', '', true)",
            &[&crate::roles::module_role(schema)],
        )
        .await?;
    let counted = transaction
        .query_one(
            &format!(
                "SELECT count(*) FROM {}.{}",
                crate::roles::quoted(schema),
                crate::roles::quoted(name)
            ),
            &[],
        )
        .await?
        .try_get::<_, i64>(0)?;
    transaction.rollback().await?;
    Ok(counted)
}

/// Hosted test 8 (ADR-0042): checks the server version, runs the catalog checks over every module schema in the
/// database, and confirms each module's role sees no tenant-owned row while no tenant is set. Returns each problem;
/// none means the rules hold. Run it as the owner role, after migrating.
///
/// # Errors
/// Fails if the server is too old or a query fails.
pub async fn check_isolation(settings: &Settings) -> Result<Vec<String>, Error> {
    require_supported_server(settings).await?;
    let mut client = connect(settings).await?;
    let schemas = module_schemas(&client).await?;
    let schemas: Vec<&str> = schemas.iter().map(String::as_str).collect();
    let mut problems = isolation_violations(&mut client, &schemas).await?;
    problems.extend(rows_seen_without_a_tenant(&mut client, &schemas).await?);
    Ok(problems)
}

/// Drops the named modules' schemas, with everything in them, their records of applied files included, so the
/// next migrate builds them afresh. Only for a database holding no real business's data, such as the demo's,
/// which the caller checks. Every name must be a module's schema here, and the server one Tetsu runs on, so a
/// release that would refuse it never finds its schemas already gone. All or none are dropped.
///
/// # Errors
/// Fails if the server is too old, a name is not a module's schema here, or the drop fails.
pub async fn drop_module_schemas(settings: &Settings, schemas: &[String]) -> Result<(), Error> {
    require_supported_server(settings).await?;
    let mut client = connect(settings).await?;
    let modules = module_schemas(&client).await?;
    if let Some(unknown) = schemas.iter().find(|schema| !modules.contains(schema)) {
        return Err(format!("{unknown:?} is not a module's schema here").into());
    }
    let transaction = client.transaction().await?;
    for schema in schemas {
        transaction
            .batch_execute(&format!("DROP SCHEMA {} CASCADE", quoted(schema)))
            .await?;
    }
    transaction.commit().await?;
    Ok(())
}
