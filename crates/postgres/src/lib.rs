//! Postgres connections (ADR-0011, ADR-0013), sorting the driver's errors (ADR-0011), the migration runner (ADR-0007, ADR-0012), tenant isolation
//! (ADR-0042), and domain events through each module's outbox (ADR-0043). A module that records events adds
//! [`OUTBOX`] as one of its migrations, and one that consumes them adds [`INBOX`] and implements [`Consumer`].

mod connection;
mod events;
mod failure;
mod isolation;
mod migrate;
mod roles;
mod scope;
#[doc(hidden)]
pub mod test_support;

pub use connection::{Error, Settings, Tls, connect, pool};
pub use deadpool_postgres::{Object, Pool, Transaction};
pub use events::{
    Applied, Consumer, Dispatcher, Event, INBOX, KEPT_FOR, MAX_ATTEMPTS, NewEvent, OUTBOX,
};
pub use failure::{Failure, STALE, clear_if_stale, sort, sort_commit, sort_pool};
pub use isolation::{
    isolation_violations, isolation_violations_for, module_schemas, rows_seen_without_a_tenant,
};
pub use migrate::{
    MIGRATION_LOCK, MIN_SERVER_VERSION, migrate, migrate_with_wait, require_supported_server,
};
pub use roles::{APP_ROLE, DELIVERY_ROLE, MAX_SCHEMA, lookup_role, module_role};
pub use scope::{
    DeliveryTransaction, LOCK_TIMEOUT, ModuleTransaction, STATEMENT_TIMEOUT, TenantTransaction,
};
