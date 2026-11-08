//! Tenancy's Postgres adapter. Driver types stay in this file (ADR-0007).

use deadpool_postgres::{Object, Transaction};
use tetsu_kernel::Migration;
use tetsu_postgres::Pool;
use tokio_postgres::Row;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::ToSql;

use crate::domain::{Color, Error, Location, NewTenant, Slug, Tenant, TenantId, Theme};
use crate::ports::TenantStore;

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("../../migrations/0001_tenants.sql"),
}];

const INSERT_TENANT: &str = include_str!("../../sql/insert_tenant.sql");
const INSERT_ADDRESS: &str = include_str!("../../sql/insert_address.sql");
const INSERT_LOCATION: &str = include_str!("../../sql/insert_location.sql");
const FIND_TENANT: &str = include_str!("../../sql/find_tenant_by_address.sql");
const FIND_LOCATIONS: &str = include_str!("../../sql/find_locations.sql");
/// Postgres says this when a cached statement no longer matches its table (ADR-0011).
const STALE: &str = "cached plan must not change result type";

/// The store the composition root builds Tenancy with.
#[must_use]
pub fn postgres_store(pool: Pool) -> impl TenantStore {
    PostgresTenants { pool }
}

struct PostgresTenants {
    pool: Pool,
}

/// Sorts driver errors as ADR-0011 asks. Only a dropped connection or a stale cached statement is worth
/// retrying, and every error keeps the driver's message.
fn database(error: &tokio_postgres::Error) -> Error {
    let message = message(error);
    if error.code() == Some(&SqlState::UNIQUE_VIOLATION) {
        Error::SlugTaken
    } else if error.is_closed() || is_io(error) || message.contains(STALE) {
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
    error.as_db_error().map_or_else(
        || {
            let causes: Vec<String> =
                std::iter::successors(Some(error as &dyn std::error::Error), |e| e.source())
                    .map(ToString::to_string)
                    .collect();
            causes.join(": ")
        },
        |db| db.message().to_string(),
    )
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
    if matches!(&result, Err(Error::Unavailable(why)) if why.contains(STALE)) {
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
        let client = self.pool.get().await.map_err(pool_error)?;
        let result = find(&client, address).await;
        clear_if_stale(&self.pool, result)
    }
}

/// One transaction, so a failure at any step leaves no tenant, address, or location behind.
async fn insert_in_transaction(client: &mut Object, tenant: &NewTenant) -> Result<i64, Error> {
    let transaction = client.transaction().await.map_err(|e| database(&e))?;
    let id = insert_tenant_and_address(&transaction, tenant).await?;
    insert_locations(&transaction, id, tenant.locations()).await?;
    transaction.commit().await.map_err(|e| commit_error(&e))?;
    Ok(id)
}

async fn insert_tenant_and_address(
    transaction: &Transaction<'_>,
    tenant: &NewTenant,
) -> Result<i64, Error> {
    let insert_tenant = transaction
        .prepare_cached(INSERT_TENANT)
        .await
        .map_err(|e| database(&e))?;
    let parameters: [&(dyn ToSql + Sync); 4] = [
        &tenant.slug().as_str(),
        &tenant.name(),
        &tenant.theme().primary.as_str(),
        &tenant.theme().accent.as_str(),
    ];
    let row = transaction
        .query_one(&insert_tenant, &parameters)
        .await
        .map_err(|e| database(&e))?;
    let id: i64 = row.try_get("id").map_err(|e| database(&e))?;
    let insert_address = transaction
        .prepare_cached(INSERT_ADDRESS)
        .await
        .map_err(|e| database(&e))?;
    transaction
        .execute(&insert_address, &[&tenant.slug().address(), &id])
        .await
        .map_err(|e| database(&e))?;
    Ok(id)
}

async fn insert_locations(
    transaction: &Transaction<'_>,
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

async fn find(client: &Object, address: &str) -> Result<Option<Tenant>, Error> {
    let find_tenant = client
        .prepare_cached(FIND_TENANT)
        .await
        .map_err(|e| database(&e))?;
    let Some(row) = client
        .query_opt(&find_tenant, &[&address])
        .await
        .map_err(|e| database(&e))?
    else {
        return Ok(None);
    };
    let id: i64 = row.try_get("id").map_err(|e| database(&e))?;
    let find_locations = client
        .prepare_cached(FIND_LOCATIONS)
        .await
        .map_err(|e| database(&e))?;
    let locations = client
        .query(&find_locations, &[&id])
        .await
        .map_err(|e| database(&e))?;
    Ok(Some(tenant_from(&row, &locations)?))
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

    /// The pool replaces a dead connection, so this case is a race in practice: test the sorting directly.
    #[tokio::test]
    async fn a_connection_the_server_closed_is_temporary() {
        let settings = tetsu_postgres::test_support::fresh_database()
            .await
            .expect("a test database");
        let client = tetsu_postgres::connect(&settings).await.expect("connects");
        let admin = tetsu_postgres::connect(&settings).await.expect("connects");
        let row = client
            .query_one("SELECT pg_backend_pid() AS pid", &[])
            .await
            .expect("finds its pid");
        let pid: i32 = row.try_get("pid").expect("maps");
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
