//! Postgres connections (ADR-0011, ADR-0013) and the migration runner (ADR-0007, ADR-0012).

mod connection;
mod isolation;
mod migrate;
mod roles;
mod scope;
#[doc(hidden)]
pub mod test_support;

pub use connection::{Error, Settings, Tls, connect, pool};
pub use deadpool_postgres::{Object, Pool};
pub use isolation::{
    isolation_violations, isolation_violations_for, module_schemas, rows_seen_without_a_tenant,
};
pub use migrate::{
    MIGRATION_LOCK, MIN_SERVER_VERSION, migrate, migrate_with_wait, require_supported_server,
};
pub use roles::{APP_ROLE, MAX_SCHEMA, lookup_role, module_role};
pub use scope::{LOCK_TIMEOUT, ModuleTransaction, STATEMENT_TIMEOUT, TenantTransaction};
