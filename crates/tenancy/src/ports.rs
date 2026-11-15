//! The driven port Tenancy calls out through.

use std::future::Future;

use crate::domain::{Error, NewTenant, Slug, Tenant};

/// Where tenants are kept. The Postgres adapter implements it, and tests may use their own.
pub trait TenantStore {
    /// Stores a tenant and registers its slug as an address.
    fn insert(&self, tenant: &NewTenant) -> impl Future<Output = Result<Tenant, Error>> + Send;

    /// Finds the tenant at an address that is already normalized.
    fn find_by_address(
        &self,
        address: &str,
    ) -> impl Future<Output = Result<Option<Tenant>, Error>> + Send;

    /// Which of these normalized addresses some tenant holds, in no particular order.
    fn find_addresses_in_use(
        &self,
        addresses: &[String],
    ) -> impl Future<Output = Result<Vec<String>, Error>> + Send;

    /// Gives the tenant at a normalized address a new slug and its address, and says whether a tenant was there.
    fn rename(&self, address: &str, to: &Slug) -> impl Future<Output = Result<bool, Error>> + Send;
}
