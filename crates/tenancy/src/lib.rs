//! Tenancy: the businesses that use Tetsu, the addresses that reach them, and their themes.
//!
//! The public surface is the use cases, the driven port, and the module.

mod adapters;
mod domain;
mod ports;
mod use_cases;

pub use adapters::postgres::postgres_store;
pub use domain::{Color, Error, Location, NewTenant, Slug, Tenant, TenantId, Theme};
pub use ports::TenantStore;
pub use use_cases::{create_tenant, resolve};

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
