//! Identity: who logs in to Tetsu, their passkeys, and their sessions (ADR-0040, ADR-0048). Admins come first,
//! in slice 5; staff and customers follow.
//!
//! The public surface (ADR-0021): the use cases on [`Accounts`], the driven ports, the domain types they name, the
//! module's registration, the adapter constructor, which returns an [`Accounts`], and the admin pages' router
//! with the paths it serves.

mod adapters;
mod browser;
mod domain;
mod mail;
mod passkeys;
mod ports;
mod use_cases;

#[cfg(test)]
mod account_tests;
#[cfg(test)]
mod adding_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod join_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod limit_tests;
#[cfg(test)]
mod login_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod sessions_tests;
#[cfg(test)]
mod staff_tests;

pub use adapters::events::notices;
pub use adapters::http::{PATHS, Web, router};
pub use adapters::postgres::{postgres_staff_store, postgres_store};
pub use browser::{Change, CheckedSession, ForChange, Logout, Origins};
pub use domain::{
    Acceptance, Account, AccountId, Actor, Addition, AdminSummary, Answer, BusinessId, Ceremony,
    CeremonyKind, CeremonyToken, Device, Disabling, Ending, Error, FailedCheck, HeldPasskey,
    Invitation, InviteToken, Invitee, Issue, Limits, ListedSession, Login, NewPasskey,
    NewStaffInvite, PersonName, RateLimit, Realm, Redemption, Refusal, Removal, Role, Session,
    SessionId, SessionStart, SessionToken, Signer, StaffInvite, StaffIssue, TakenAddition,
    TakenCeremony, TimeLeft,
};
pub use mail::{invite_message, staff_invite_message};
pub use passkeys::RelyingParty;
pub use ports::{AdminDirectory, AdminStore, LoginStore, StaffDirectory, StaffStore};
pub use use_cases::Accounts;

/// Registers Identity with the kernel.
pub struct Identity;

impl tetsu_kernel::Module for Identity {
    fn name(&self) -> &'static str {
        "identity"
    }

    fn storage(&self) -> Option<tetsu_kernel::Storage> {
        Some(tetsu_kernel::Storage {
            schema: "identity",
            migrations: adapters::postgres::MIGRATIONS.as_slice(),
        })
    }
}
