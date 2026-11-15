//! Identity: who signs in to Tetsu, their passkeys, and their sessions (ADR-0040, ADR-0048). Operators come first,
//! in slice 5; staff and customers follow.
//!
//! The public surface (ADR-0021): the use cases on [`Operators`], the driven port, the domain types they name, the
//! module's registration, and the adapter constructor, which returns an [`Operators`].

mod adapters;
mod browser;
mod domain;
mod ports;
mod use_cases;

pub use adapters::postgres::postgres_store;
pub use browser::{Change, ForChange, Origins, SignOut};
pub use domain::{
    EnrollmentToken, Error, Limits, OperatorId, OperatorName, OperatorSummary, Realm, Session,
    SessionToken,
};
pub use ports::OperatorStore;
pub use use_cases::Operators;

/// Registers Identity with the kernel.
pub struct Identity;

impl tetsu_kernel::Module for Identity {
    fn name(&self) -> &'static str {
        "identity"
    }

    fn storage(&self) -> Option<tetsu_kernel::Storage> {
        Some(tetsu_kernel::Storage {
            schema: "identity",
            migrations: adapters::postgres::MIGRATIONS,
        })
    }
}
