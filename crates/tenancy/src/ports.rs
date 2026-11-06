//! The driven port Tenancy calls out through.

use std::future::Future;

use crate::domain::{Error, NewTenant, Tenant};

/// Where tenants are kept. The Postgres adapter implements it, and tests may use their own.
pub trait TenantStore {
    /// Stores a tenant and registers its slug as an address.
    fn insert(&self, tenant: &NewTenant) -> impl Future<Output = Result<Tenant, Error>> + Send;

    /// Finds the tenant at an address that is already normalized.
    fn find_by_address(
        &self,
        address: &str,
    ) -> impl Future<Output = Result<Option<Tenant>, Error>> + Send;
}
