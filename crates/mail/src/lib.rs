//! Mail: the `Mailer` port's adapters (ADR-0044, ADR-0074). A message to an address on the allowlist goes out by
//! SMTP when a server is set; every other message is kept for the operators' mailbox page. Either way it goes once
//! per key.
//!
//! The public surface (ADR-0021): the module's registration, [`Mail`]; the adapter constructor, [`post_office`],
//! which returns a [`PostOffice`], the kernel's `Mailer` and `Mailbox`; the SMTP adapter, [`Smtp`], and its
//! settings; and the [`Allowlist`].

mod adapters;
mod domain;

pub use adapters::postgres::{PostOffice, post_office};
pub use adapters::smtp::{Refused, Smtp, SmtpSettings};
pub use domain::Allowlist;

use tetsu_kernel::{Migration, Module, Storage};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("../migrations/0001_mail.sql"),
}];

/// Registers Mail with the kernel.
pub struct Mail;

impl Module for Mail {
    fn name(&self) -> &'static str {
        "mail"
    }

    fn storage(&self) -> Option<Storage> {
        Some(Storage {
            schema: "mail",
            migrations: MIGRATIONS,
        })
    }
}
