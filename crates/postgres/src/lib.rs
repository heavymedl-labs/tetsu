//! Postgres connections (ADR-0011, ADR-0013), sorting the driver's errors (ADR-0011), the migration runner (ADR-0007, ADR-0012), tenant isolation
//! (ADR-0042), and domain events through each module's outbox (ADR-0043). A module that records events adds
//! [`OUTBOX`] as one of its migrations, and one that consumes them adds [`INBOX`] then [`INBOX_DELIVERY`] and
//! implements [`Consumer`]. The dispatcher removes rows past their use each hour (ADR-0080): delivered events after
//! [`KEPT_FOR`], events that stopped retrying after [`STOPPED_KEPT_FOR`], and inbox IDs after [`APPLIED_KEPT_FOR`]
//! once no outbox holds their event.

mod connection;
mod database;
mod events;
mod failure;
mod isolation;
mod migrate;
mod render;
mod roles;
mod scope;
#[doc(hidden)]
pub mod test_support;

pub use connection::{Error, Settings, Tls, connect, pool};
pub use database::create_database;
pub use deadpool_postgres::{Object, Pool, Transaction};
pub use events::{
    APPLIED_KEPT_FOR, Applied, Consumer, Dispatcher, Event, INBOX, INBOX_DELIVERY, KEPT_FOR,
    MAX_ATTEMPTS, NewEvent, OUTBOX, Removed, STOPPED_KEPT_FOR,
};
pub use failure::{Failure, STALE, clear_if_stale, sort, sort_commit, sort_pool};
pub use isolation::{
    check_isolation, drop_module_schemas, isolation_violations, isolation_violations_for,
    module_schemas, rows_seen_without_a_tenant,
};
pub use migrate::{
    Behind, MIGRATION_LOCK, MIN_SERVER_VERSION, behind_code, migrate, migrate_with_wait,
    require_supported_server,
};
pub use render::{leftovers, render};
pub use roles::{APP_ROLE, DELIVERY_ROLE, MAX_SCHEMA, lookup_role, module_role};
pub use scope::{
    DeliveryTransaction, LOCK_TIMEOUT, ModuleTransaction, STATEMENT_TIMEOUT, TenantTransaction,
};
