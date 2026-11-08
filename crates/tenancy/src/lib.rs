//! Tenancy: the businesses that use Tetsu, the addresses that reach them, and their themes.
//!
//! The public surface (ADR-0021): the use cases on [`Tenants`], the driven port, the domain types they name, the
//! module's registration, and the adapter constructor, which returns a [`Tenants`].

mod adapters;
mod domain;
mod ports;
mod use_cases;

pub use adapters::postgres::postgres_store;
pub use domain::{Color, Error, Location, NewTenant, Slug, Tenant, TenantId, Theme};
pub use ports::TenantStore;
pub use use_cases::Tenants;

/// Registers Tenancy with the kernel.
pub struct Tenancy;

impl tetsu_kernel::Module for Tenancy {
    fn name(&self) -> &'static str {
        "tenancy"
    }

    fn storage(&self) -> Option<tetsu_kernel::Storage> {
        Some(tetsu_kernel::Storage {
            schema: "tenancy",
            migrations: adapters::postgres::MIGRATIONS,
        })
    }
}
