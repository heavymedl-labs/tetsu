//! Audit: the record of security events, such as sign-ins and recoveries, kept per tenant and never lost
//! (ADR-0045). It builds the record from domain events (ADR-0043), and keeps events that belong to no business,
//! such as an operator's sign-in, in a global record.
//!
//! The public surface (ADR-0021): the module's registration, [`Audit`], and the adapter constructor,
//! [`consumer`], which the composition root hands to the dispatcher.

mod consumer;

pub use consumer::consumer;

use tetsu_kernel::{Migration, Module, Storage};

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../migrations/0001_records.sql"),
    },
    Migration {
        version: 2,
        sql: tetsu_postgres::INBOX,
    },
];

/// Registers Audit with the kernel.
pub struct Audit;

impl Module for Audit {
    fn name(&self) -> &'static str {
        "audit"
    }

    fn storage(&self) -> Option<Storage> {
        Some(Storage {
            schema: "audit",
            migrations: MIGRATIONS,
        })
    }
}
