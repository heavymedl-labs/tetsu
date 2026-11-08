//! Use cases, reached only through [`Tenants`] (ADR-0021).

use crate::domain::{Error, NewTenant, Tenant};
use crate::ports::TenantStore;

/// Tenancy's use cases over a store. It offers nothing that reaches the store, so a caller outside Tenancy can
/// never skip a use case, such as `resolve`'s checks, by calling the port.
///
/// The use cases are there:
///
/// ```no_run
/// # async fn reach(pool: tetsu_postgres::Pool) {
/// let _ = tetsu_tenancy::postgres_store(pool).resolve("/x").await;
/// # }
/// ```
///
/// The port is not, even with it imported:
///
/// ```compile_fail,E0599
/// # async fn bypass(pool: tetsu_postgres::Pool) {
/// use tetsu_tenancy::TenantStore;
/// let _ = tetsu_tenancy::postgres_store(pool).find_by_address("/x").await;
/// # }
/// ```
///
/// Nor is the store inside it. A method that returned the store would break this too, and only review can catch
/// one, since it has no name to test for.
///
/// ```compile_fail,E0616
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// let _ = &tetsu_tenancy::postgres_store(pool).store;
/// # }
/// ```
pub struct Tenants<S> {
    store: S,
}

impl<S: TenantStore> Tenants<S> {
    /// Wraps a store, such as a test's own or another adapter.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Creates a tenant, reachable at its slug.
    ///
    /// # Errors
    /// Fails if the slug is taken or storage fails.
    pub async fn create(&self, tenant: &NewTenant) -> Result<Tenant, Error> {
        self.store.insert(tenant).await
    }

    /// Finds the tenant at an address. An unknown address fails, and never falls back to a default tenant.
    ///
    /// # Errors
    /// Fails with [`Error::NotFound`] for an unknown address, including one holding a control character, which
    /// no address does. Fails if storage fails.
    pub async fn resolve(&self, address: &str) -> Result<Tenant, Error> {
        if address.chars().any(char::is_control) {
            return Err(Error::NotFound);
        }
        // Only ASCII whitespace, which no address holds: other whitespace, such as U+2028, makes it unknown.
        let address = address
            .trim_matches(|c: char| c.is_ascii_whitespace())
            .to_ascii_lowercase();
        self.store
            .find_by_address(&address)
            .await?
            .ok_or(Error::NotFound)
    }
}
