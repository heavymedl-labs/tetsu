//! Use cases, reached only through [`Tenants`] (ADR-0021).

use crate::domain::{Error, NewTenant, Tenant};
use crate::ports::TenantStore;

/// Tenancy's use cases over a store. It offers nothing that reaches the store, so a caller outside Tenancy can
/// never skip a use case, such as `resolve`'s checks, through it. Tenancy's tables stay reachable through the
/// database pool, which the rule that a module touches only its own schema governs (#8). The doc tests on
/// `postgres_store` rule out a store impl, a deref, and a public field. Review catches any other route to the
/// store, such as a method or a conversion trait that hands it out.
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
