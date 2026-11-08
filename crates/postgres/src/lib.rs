//! Postgres connections (ADR-0011, ADR-0013) and the migration runner (ADR-0007, ADR-0012).

mod connection;
mod migrate;
#[doc(hidden)]
pub mod test_support;

pub use connection::{Error, Settings, Tls, connect, pool};
pub use deadpool_postgres::{Object, Pool};
pub use migrate::{MIGRATION_LOCK, migrate, migrate_with_wait, require_utf8};
