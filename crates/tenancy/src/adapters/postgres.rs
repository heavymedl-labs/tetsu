//! Tenancy's Postgres adapter. Driver types stay in this file (ADR-0007).

use deadpool_postgres::Object;
use tetsu_kernel::Migration;
use tetsu_postgres::{Failure, ModuleTransaction, Pool, TenantTransaction};
use tokio_postgres::Row;
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

/// Sorts driver errors through `tetsu_postgres` (ADR-0011). A taken slug and a taken address are told apart by the
/// constraint each breaks.
fn database(error: &tokio_postgres::Error) -> Error {
    from_failure(tetsu_postgres::sort(error))
}

fn commit_error(error: &tokio_postgres::Error) -> Error {
    from_failure(tetsu_postgres::sort_commit(error))
}

fn pool_error(error: deadpool_postgres::PoolError) -> Error {
    from_failure(tetsu_postgres::sort_pool(error))
}

fn from_failure(failure: Failure) -> Error {
    match failure {
        Failure::Taken {
            constraint,
            message,
        } => match constraint.as_deref() {
            Some("tenants_slug_key") => Error::SlugTaken,
            Some("addresses_pkey") => Error::AddressTaken,
            _ => Error::Storage(message),
        },
        Failure::Temporary(why) => Error::Unavailable(why),
        Failure::Permanent(why) => Error::Storage(why),
    }
}

/// Clears every pooled connection's statement cache after a stale statement, before the next request.
fn clear_if_stale<T>(pool: &Pool, result: Result<T, Error>) -> Result<T, Error> {
    if let Err(Error::Unavailable(why)) = &result {
        tetsu_postgres::clear_if_stale(pool, why);
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

/// One statement, which still asks the lookup about one address a call, as it allows.
async fn addresses_in_use(client: &mut Object, addresses: &[String]) -> Result<Vec<String>, Error> {
    let transaction = ModuleTransaction::begin(client, SCHEMA)
        .await
        .map_err(|e| database(&e))?;
    let statement = transaction
        .prepare_cached(FIND_ADDRESSES_IN_USE)
        .await
        .map_err(|e| database(&e))?;
    let held = transaction
        .query(&statement, &[&addresses])
        .await
        .map_err(|e| database(&e))?
        .iter()
        .map(|row| row.try_get("address").map_err(|e| database(&e)))
        .collect::<Result<_, _>>()?;
    transaction.commit().await.map_err(|e| commit_error(&e))?;
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
