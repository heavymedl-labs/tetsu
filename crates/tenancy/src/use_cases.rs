//! Use cases.

use crate::domain::{Error, NewTenant, Tenant};
use crate::ports::TenantStore;

/// Creates a tenant, reachable at its slug.
///
/// # Errors
/// Fails if the slug is taken or storage fails.
pub async fn create_tenant(store: &impl TenantStore, tenant: &NewTenant) -> Result<Tenant, Error> {
    store.insert(tenant).await
}

/// Finds the tenant at an address. An unknown address fails, and never falls back to a default tenant.
///
/// # Errors
/// Fails with [`Error::NotFound`] for an unknown address, including one holding a control character, which no
/// address does. Fails if storage fails.
pub async fn resolve(store: &impl TenantStore, address: &str) -> Result<Tenant, Error> {
    if address.chars().any(char::is_control) {
        return Err(Error::NotFound);
    }
    // Only ASCII whitespace, which no address holds: other whitespace, such as U+2028, makes it unknown.
    let address = address
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .to_ascii_lowercase();
    store
        .find_by_address(&address)
        .await?
        .ok_or(Error::NotFound)
}
