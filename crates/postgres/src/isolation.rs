//! The catalog checks ADR-0042 asks for: every way the modules' tables, policies, and roles break its rules.

use tokio_postgres::Client;

use crate::connection::Error;
use crate::roles::APP_ROLE;

const VIOLATIONS: &str = include_str!("../sql/isolation_violations.sql");

/// Each problem the catalog shows in these schemas, in a stable order; none means the rules hold. Run it as any
/// role that can read the catalog, after migrating.
///
/// # Errors
/// Fails if the query fails.
pub async fn isolation_violations(client: &Client, schemas: &[&str]) -> Result<Vec<String>, Error> {
    isolation_violations_for(client, schemas, APP_ROLE).await
}

/// [`isolation_violations`] for another application role, so a test can break the rules without touching the
/// role every test database shares.
///
/// # Errors
/// Fails if the query fails.
#[doc(hidden)]
pub async fn isolation_violations_for(
    client: &Client,
    schemas: &[&str],
    app: &str,
) -> Result<Vec<String>, Error> {
    let schemas: Vec<String> = schemas.iter().map(ToString::to_string).collect();
    // In a transaction whose search path is the catalog alone, so no object a module made can stand in for a
    // function the checks call or print.
    client.batch_execute("BEGIN").await?;
    let rows = async {
        client
            .query_one(
                "SELECT pg_catalog.set_config('search_path', 'pg_catalog', true)",
                &[],
            )
            .await?;
        client.query(VIOLATIONS, &[&schemas, &app]).await
    }
    .await;
    // Rolled back whatever happened, and the query's own error, if any, is the one reported.
    let rolled_back = client.batch_execute("ROLLBACK").await;
    let rows = rows?;
    rolled_back?;
    Ok(rows
        .iter()
        .map(|row| row.try_get("problem"))
        .collect::<Result<_, _>>()?)
}

/// Every module's schema in the database, found by the record each keeps of its applied files.
///
/// # Errors
/// Fails if the query fails.
pub async fn module_schemas(client: &Client) -> Result<Vec<String>, Error> {
    let rows = client
        .query(
            "SELECT n.nspname FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE c.relname = 'migrations' AND c.relkind = 'r' ORDER BY 1",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| row.try_get(0))
        .collect::<Result<_, _>>()?)
}

/// Each tenant-owned table in these schemas that shows its module's role any row while no tenant is set, which
/// row-level security forbids (ADR-0042). Every count runs in a transaction that is rolled back.
///
/// # Errors
/// Fails if a query fails, such as when the connection's role may not act as a module's.
pub async fn rows_seen_without_a_tenant(
    client: &Client,
    schemas: &[&str],
) -> Result<Vec<String>, Error> {
    let schemas: Vec<String> = schemas.iter().map(ToString::to_string).collect();
    let tables = client
        .query(
            "SELECT n.nspname, c.relname FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = any($1) AND c.relkind IN ('r', 'p')
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
async fn count_as_module(client: &Client, schema: &str, name: &str) -> Result<i64, Error> {
    client.batch_execute("BEGIN").await?;
    let counted = async {
        client
            .query_one(
                "SELECT pg_catalog.set_config('role', $1, true),
                        pg_catalog.set_config('app.tenant_id', '', true)",
                &[&crate::roles::module_role(schema)],
            )
            .await?;
        client
            .query_one(
                &format!(
                    "SELECT count(*) FROM {}.{}",
                    crate::roles::quoted(schema),
                    crate::roles::quoted(name)
                ),
                &[],
            )
            .await?
            .try_get::<_, i64>(0)
    }
    .await;
    let rolled_back = client.batch_execute("ROLLBACK").await;
    let counted = counted?;
    rolled_back?;
    Ok(counted)
}
