//! Tenancy's Postgres adapter. Driver types stay in this file (ADR-0007).

use deadpool_postgres::Object;
use tetsu_kernel::Migration;
use tetsu_postgres::{ModuleTransaction, Pool, TenantTransaction};
use tokio_postgres::Row;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::ToSql;

use crate::domain::{Color, Error, Location, NewTenant, Slug, Tenant, TenantId, Theme};
use crate::ports::TenantStore;
use crate::use_cases::Tenants;

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../migrations/0001_tenants.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("../../migrations/0002_isolation.sql"),
    },
];
/// Tenancy's schema, which names the role its transactions act as (ADR-0042).
const SCHEMA: &str = "tenancy";

const REGISTER_TENANT: &str = include_str!("../../sql/register_tenant.sql");
const INSERT_LOCATION: &str = include_str!("../../sql/insert_location.sql");
const FIND_TENANT: &str = include_str!("../../sql/find_tenant_by_address.sql");
const FIND_LOCATIONS: &str = include_str!("../../sql/find_locations.sql");
const FIND_ADDRESSES_IN_USE: &str = include_str!("../../sql/find_addresses_in_use.sql");
/// Marks an error from a cached statement that no longer matches its table (ADR-0011).
const STALE: &str = "stale cached statement: ";
/// The Postgres function that raises it, which unlike the message does not change with the server's language.
const STALE_ROUTINE: &str = "RevalidateCachedQuery";

/// Tenancy over Postgres, for the composition root. It returns the use cases, never the store itself
/// (ADR-0021).
///
/// The use cases are there:
///
/// ```no_run
/// # async fn reach(pool: tetsu_postgres::Pool) {
/// let _ = tetsu_tenancy::postgres_store(pool).resolve("/x").await;
/// # }
/// ```
///
/// The handle is not a store, whatever the port's methods are called:
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn store(_: &impl tetsu_tenancy::TenantStore) {}
/// store(&tetsu_tenancy::postgres_store(pool));
/// # }
/// ```
///
/// It derefs to nothing, so no store's methods are reached through it:
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn derefs(_: &impl std::ops::Deref) {}
/// derefs(&tetsu_tenancy::postgres_store(pool));
/// # }
/// ```
///
/// And the store inside it is private:
///
/// ```compile_fail,E0616
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// let _ = &tetsu_tenancy::postgres_store(pool).store;
/// # }
/// ```
#[must_use]
pub fn postgres_store(pool: Pool) -> Tenants<impl TenantStore> {
    Tenants::new(PostgresTenants { pool })
}

struct PostgresTenants {
    pool: Pool,
}

/// Sorts driver errors as ADR-0011 asks. Only a dropped connection or a stale cached statement is worth
/// retrying, and every error keeps the driver's message. A taken slug and a taken address are told apart by the
/// constraint each breaks.
fn database(error: &tokio_postgres::Error) -> Error {
    let message = message(error);
    if error.code() == Some(&SqlState::UNIQUE_VIOLATION) {
        match error.as_db_error().and_then(|db| db.constraint()) {
            Some("tenants_slug_key") => Error::SlugTaken,
            Some("addresses_pkey") => Error::AddressTaken,
            _ => Error::Storage(message),
        }
    } else if is_stale(error) {
        Error::Unavailable(format!("{STALE}{message}"))
    } else if error.is_closed() || is_io(error) || is_server_unavailable(error) {
        Error::Unavailable(message)
    } else {
        Error::Storage(message)
    }
}

/// A commit Postgres answered was refused and rolled back, so it is sorted like any other error. One with no
/// answer may still have been saved, so a retry is not safe: it is never temporary (ADR-0011).
fn commit_error(error: &tokio_postgres::Error) -> Error {
    if error.as_db_error().is_none() {
        return Error::Storage(format!("commit outcome unknown: {}", message(error)));
    }
    match database(error) {
        Error::Storage(why) | Error::Unavailable(why) => {
            Error::Storage(format!("commit refused: {why}"))
        }
        other => other,
    }
}

/// The driver's own text is just "db error" or "error connecting to server", so this keeps what Postgres said,
/// or the cause underneath, such as a refused connection.
fn message(error: &tokio_postgres::Error) -> String {
    if let Some(db) = error.as_db_error() {
        return db.message().to_string();
    }
    let causes: Vec<String> =
        std::iter::successors(Some(error as &dyn std::error::Error), |e| e.source())
            .map(ToString::to_string)
            .collect();
    causes.join(": ")
}

/// The server ending the session for a restart, crash, or failover, still starting up, or out of connections.
/// A query running at that moment gets one of these, not a closed connection.
fn is_server_unavailable(error: &tokio_postgres::Error) -> bool {
    error.code().is_some_and(|code| {
        [
            SqlState::ADMIN_SHUTDOWN,
            SqlState::CRASH_SHUTDOWN,
            SqlState::CANNOT_CONNECT_NOW,
            SqlState::TOO_MANY_CONNECTIONS,
        ]
        .contains(code)
    })
}

fn is_stale(error: &tokio_postgres::Error) -> bool {
    error.as_db_error().is_some_and(|db| {
        db.code() == &SqlState::FEATURE_NOT_SUPPORTED && db.routine() == Some(STALE_ROUTINE)
    })
}

fn is_io(error: &tokio_postgres::Error) -> bool {
    std::error::Error::source(error).is_some_and(<dyn std::error::Error>::is::<std::io::Error>)
}

/// A pool timeout or a lost connection is worth retrying. A refused login or bad settings is not.
fn pool_error(error: deadpool_postgres::PoolError) -> Error {
    match error {
        deadpool_postgres::PoolError::Timeout(_) => Error::Unavailable(error.to_string()),
        deadpool_postgres::PoolError::Backend(error) => database(&error),
        other => Error::Storage(other.to_string()),
    }
}

/// A stale statement stays cached until the caches are cleared. Every pooled connection may hold it,
/// so clear them all before the next request.
fn clear_if_stale<T>(pool: &Pool, result: Result<T, Error>) -> Result<T, Error> {
    if matches!(&result, Err(Error::Unavailable(why)) if why.starts_with(STALE)) {
        pool.manager().statement_caches.clear();
    }
    result
}

impl TenantStore for PostgresTenants {
    async fn insert(&self, tenant: &NewTenant) -> Result<Tenant, Error> {
        let mut client = self.pool.get().await.map_err(pool_error)?;
        let result = insert_in_transaction(&mut client, tenant).await;
        clear_if_stale(
            &self.pool,
            result.map(|id| tenant.stored_as(TenantId::new(id))),
        )
    }

    async fn find_by_address(&self, address: &str) -> Result<Option<Tenant>, Error> {
        let mut client = self.pool.get().await.map_err(pool_error)?;
        let result = find(&mut client, address).await;
        clear_if_stale(&self.pool, result)
    }

    async fn find_addresses_in_use(&self, addresses: &[String]) -> Result<Vec<String>, Error> {
        let mut client = self.pool.get().await.map_err(pool_error)?;
        let result = addresses_in_use(&mut client, addresses).await;
        clear_if_stale(&self.pool, result)
    }
}

/// Asks about one address at a time, as the lookup allows, in one transaction.
async fn addresses_in_use(client: &mut Object, addresses: &[String]) -> Result<Vec<String>, Error> {
    let transaction = ModuleTransaction::begin(client, SCHEMA)
        .await
        .map_err(|e| database(&e))?;
    let statement = transaction
        .prepare_cached(FIND_ADDRESSES_IN_USE)
        .await
        .map_err(|e| database(&e))?;
    let mut held = Vec::new();
    for address in addresses {
        let row = transaction
            .query_one(&statement, &[address])
            .await
            .map_err(|e| database(&e))?;
        if row.try_get("held").map_err(|e| database(&e))? {
            held.push(address.clone());
        }
    }
    transaction.commit().await.map_err(|e| commit_error(&e))?;
    held.sort();
    Ok(held)
}

/// One transaction, so a failure at any step leaves no tenant, address, or location behind. The locations are
/// written once the transaction is narrowed to the new tenant, as their policy requires.
async fn insert_in_transaction(client: &mut Object, tenant: &NewTenant) -> Result<i64, Error> {
    let transaction = ModuleTransaction::begin(client, SCHEMA)
        .await
        .map_err(|e| database(&e))?;
    let id = register_tenant(&transaction, tenant).await?;
    let transaction = transaction.for_tenant(id).await.map_err(|e| database(&e))?;
    insert_locations(&transaction, id, tenant.locations()).await?;
    transaction.commit().await.map_err(|e| commit_error(&e))?;
    Ok(id)
}

/// Registers the tenant and its address through Tenancy's function, the only way its role reaches either table.
async fn register_tenant(
    transaction: &ModuleTransaction<'_>,
    tenant: &NewTenant,
) -> Result<i64, Error> {
    let register = transaction
        .prepare_cached(REGISTER_TENANT)
        .await
        .map_err(|e| database(&e))?;
    let parameters: [&(dyn ToSql + Sync); 5] = [
        &tenant.slug().as_str(),
        &tenant.name(),
        &tenant.theme().primary.as_str(),
        &tenant.theme().accent.as_str(),
        &tenant.slug().address(),
    ];
    let row = transaction
        .query_one(&register, &parameters)
        .await
        .map_err(|e| database(&e))?;
    row.try_get("id").map_err(|e| database(&e))
}

async fn insert_locations(
    transaction: &TenantTransaction<'_>,
    id: i64,
    locations: &[Location],
) -> Result<(), Error> {
    let insert_location = transaction
        .prepare_cached(INSERT_LOCATION)
        .await
        .map_err(|e| database(&e))?;
    for (order, location) in (0_i64..).zip(locations) {
        let parameters: [&(dyn ToSql + Sync); 4] =
            [&id, &order, &location.name(), &location.time_zone()];
        transaction
            .execute(&insert_location, &parameters)
            .await
            .map_err(|e| database(&e))?;
    }
    Ok(())
}

/// The lookup runs in its own short transaction, before the tenant is known, then the locations in one that
/// belongs to that tenant (ADR-0042).
async fn find(client: &mut Object, address: &str) -> Result<Option<Tenant>, Error> {
    let Some(row) = look_up(client, address).await? else {
        return Ok(None);
    };
    let id: i64 = row.try_get("id").map_err(|e| database(&e))?;
    let transaction = TenantTransaction::begin(client, SCHEMA, id)
        .await
        .map_err(|e| database(&e))?;
    let find_locations = transaction
        .prepare_cached(FIND_LOCATIONS)
        .await
        .map_err(|e| database(&e))?;
    let locations = transaction
        .query(&find_locations, &[&id])
        .await
        .map_err(|e| database(&e))?;
    transaction.commit().await.map_err(|e| commit_error(&e))?;
    Ok(Some(tenant_from(&row, &locations)?))
}

async fn look_up(client: &mut Object, address: &str) -> Result<Option<Row>, Error> {
    let transaction = ModuleTransaction::begin(client, SCHEMA)
        .await
        .map_err(|e| database(&e))?;
    let find_tenant = transaction
        .prepare_cached(FIND_TENANT)
        .await
        .map_err(|e| database(&e))?;
    let row = transaction
        .query_opt(&find_tenant, &[&address])
        .await
        .map_err(|e| database(&e))?;
    transaction.commit().await.map_err(|e| commit_error(&e))?;
    Ok(row)
}

/// Stored values pass the same checks as new ones, so a bad row is an error rather than a bad page.
fn tenant_from(row: &Row, locations: &[Row]) -> Result<Tenant, Error> {
    let locations = locations
        .iter()
        .map(location_from)
        .collect::<Result<_, Error>>()?;
    let new = NewTenant::new(
        Slug::parse(&text(row, "slug")?)?,
        &text(row, "name")?,
        theme_from(row)?,
        locations,
    )?;
    Ok(new.stored_as(TenantId::new(row.try_get("id").map_err(|e| database(&e))?)))
}

fn theme_from(row: &Row) -> Result<Theme, Error> {
    Ok(Theme {
        primary: Color::parse(&text(row, "primary_color")?)?,
        accent: Color::parse(&text(row, "accent_color")?)?,
    })
}

fn location_from(row: &Row) -> Result<Location, Error> {
    Location::new(&text(row, "name")?, &text(row, "time_zone")?)
}

fn text(row: &Row, column: &str) -> Result<String, Error> {
    row.try_get(column).map_err(|e| database(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A connection, a second one to end it with, and the first one's server process id.
    async fn client_and_its_killer() -> (
        tetsu_postgres::test_support::TestDatabase,
        tokio_postgres::Client,
        tokio_postgres::Client,
        i32,
    ) {
        let settings = tetsu_postgres::test_support::fresh_database()
            .await
            .expect("a test database");
        let client = tetsu_postgres::connect(&settings).await.expect("connects");
        let admin = tetsu_postgres::connect(&settings).await.expect("connects");
        let row = client
            .query_one("SELECT pg_backend_pid() AS pid", &[])
            .await
            .expect("finds its pid");
        let pid = row.try_get("pid").expect("maps");
        (settings, client, admin, pid)
    }

    /// The pool replaces a dead connection, so this case is a race in practice: test the sorting directly.
    #[tokio::test]
    async fn a_connection_the_server_closed_is_temporary() {
        let (_database, client, admin, pid) = client_and_its_killer().await;
        admin
            .execute("SELECT pg_terminate_backend($1)", &[&pid])
            .await
            .expect("ends the connection");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let error = client
            .query_one("SELECT 1", &[])
            .await
            .expect_err("the connection is closed");
        assert!(matches!(database(&error), Error::Unavailable(_)), "{error}");
    }

    /// A restart or failover ends sessions with a FATAL error that reaches a query still running, not a closed
    /// connection, so it must be sorted as temporary too.
    #[tokio::test]
    async fn a_query_the_server_cuts_off_is_temporary() {
        let (_database, client, admin, pid) = client_and_its_killer().await;
        let running =
            tokio::spawn(async move { client.query_one("SELECT pg_sleep(10)", &[]).await });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        admin
            .execute("SELECT pg_terminate_backend($1)", &[&pid])
            .await
            .expect("ends the session");
        let error = running
            .await
            .expect("the query task ends")
            .expect_err("the query is cut off");
        assert_eq!(error.code(), Some(&SqlState::ADMIN_SHUTDOWN), "{error}");
        assert!(matches!(database(&error), Error::Unavailable(_)), "{error}");
    }

    #[tokio::test]
    async fn only_a_stale_plan_counts_among_unsupported_features() {
        let settings = tetsu_postgres::test_support::fresh_database()
            .await
            .expect("a test database");
        let client = tetsu_postgres::connect(&settings).await.expect("connects");
        let error = client
            .query("SELECT count(*) FROM pg_class FOR UPDATE", &[])
            .await
            .expect_err("unsupported");
        assert_eq!(error.code(), Some(&SqlState::FEATURE_NOT_SUPPORTED));
        assert!(matches!(database(&error), Error::Storage(_)), "{error}");
    }
}
