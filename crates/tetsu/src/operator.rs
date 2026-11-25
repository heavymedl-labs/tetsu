//! What the commands that act for an operator share (ADR-0040, ADR-0061): who ran one, where the invite links it
//! issues point, and what came of emailing one. An invite link is printed only to a terminal, never to a pipe, so it
//! cannot land in CI's output or a log by accident, and emailed only to the address it invites.

use std::env::VarError;
use std::io::Read;

use tetsu_identity::{Actor, InviteToken};
use tetsu_kernel::{Address, Delivery, MailError};

use crate::settings::{self, Local};

/// Why a command that issues a link refuses to run where its output is not a terminal.
pub const TERMINAL_ONLY: &str = "an invite link prints only to a terminal, never to a log or a pipe (ADR-0040); run this in one";

/// Who runs a command, and where the links it issues point.
pub struct Acting {
    pub by: Actor,
    pub origin: String,
}

/// Who runs this command, as the machine's login and host name say.
pub fn who() -> Actor {
    Actor::from_login(
        login(|name| std::env::var(name)).as_deref(),
        read_small("/etc/hostname").as_deref().and_then(host_name),
    )
}

/// An invite link to the join page at `path`, with the token in the fragment, which a browser never sends to the
/// server or in a `Referer`, so it stays out of the server's logs. A browser's history keeps it, and a container run
/// with a terminal (`docker run -t`) logs it, which is why it is good once and short-lived.
pub fn link_url(origin: &str, path: &str, token: &InviteToken) -> String {
    format!("{origin}{path}#{}", token.to_text())
}

/// Where a link points. A link holds a live token, so a `.env` may name the origin only when it names the database
/// too, and never point a real database's links at another site.
pub fn link_origin(local: &Local) -> Result<String, String> {
    local.beside("DATABASE_URL", &["TETSU_ORIGIN"])?;
    settings::origin(|name| local.var(name))
}

/// Who is logged in to run the command, read from the process's own environment and never from `.env`, which
/// holds settings, not who someone is: `SUDO_USER` first, so `sudo` does not hide the person, then `USER`, then
/// `LOGNAME`, each skipped when unset or blank. A value that is not UTF-8 names no one, rather than falling through
/// to the next. It is asserted, not proven: database access is what grants the command (ADR-0061).
fn login(var: impl Fn(&str) -> Result<String, VarError>) -> Option<String> {
    for name in ["SUDO_USER", "USER", "LOGNAME"] {
        match var(name) {
            Ok(value) if !value.trim().is_empty() => return Some(value),
            Err(VarError::NotUnicode(_)) => return None,
            _ => {}
        }
    }
    None
}

/// The text of a small regular file, or none for one that is missing, unreadable, not a regular file, such as a
/// pipe, or larger than a host name could be, so reading it can never hang the command.
fn read_small(path: impl AsRef<std::path::Path>) -> Option<String> {
    const MOST: u64 = 4096;
    // Checked before opening, since opening a pipe waits for a writer.
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MOST {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MOST)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// The machine's name from `/etc/hostname`: its first line that is neither blank nor a `#` comment.
fn host_name(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
}

/// What came of emailing an invite to `to`.
pub fn delivered(to: &Address, sent: Result<Delivery, MailError>) -> String {
    let to = to.as_str();
    let still = "the link above still works";
    match sent {
        Ok(Delivery::Sent) => format!("emailed the invite link to {to}"),
        Ok(Delivery::Repeated) => format!("the invite for {to} was handed over before"),
        Ok(Delivery::Withheld) => format!(
            "did not email the invite to {to}, since an invite goes only by a mail server to an address on TETSU_MAIL_ALLOW, and is never kept; {still}"
        ),
        // An invite is never kept, so this would be a fault in Mail, said as one.
        Ok(Delivery::Captured) => format!(
            "kept the invite for {to} in the mailbox, which should never happen; disable whoever it invites and report it"
        ),
        Err(MailError::Unavailable) => {
            format!("could not email the invite link to {to} just now; {still}")
        }
        Err(MailError::Refused) => format!("the mail server refused the invite for {to}; {still}"),
    }
}

/// A fresh database migrated for Identity and Mail, a pool over it as the application, and a post office that keeps
/// every message, for tests of the commands that email, kept while the test runs.
#[cfg(test)]
pub async fn mailing() -> (
    tetsu_postgres::Pool,
    tetsu_mail::PostOffice,
    tetsu_postgres::test_support::TestDatabase,
) {
    let database = tetsu_postgres::test_support::migrated_database(&[
        &tetsu_identity::Identity,
        &tetsu_mail::Mail,
    ])
    .await
    .unwrap();
    let pool = tetsu_postgres::pool(&database.as_app()).unwrap();
    let office = tetsu_mail::post_office(pool.clone(), None, tetsu_mail::Allowlist::default());
    (pool, office, database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetsu_postgres::test_support::Scratch;

    /// Each way an invite's email can go has its own words, and every failure says the link still works.
    #[test]
    fn each_delivery_says_what_came_of_it() {
        let to = Address::parse("ada@example.com").unwrap();
        for (sent, said) in [
            (
                Ok(Delivery::Sent),
                "emailed the invite link to ada@example.com",
            ),
            (
                Ok(Delivery::Captured),
                "kept the invite for ada@example.com in the mailbox, which should never happen; disable whoever it invites and report it",
            ),
            (
                Ok(Delivery::Repeated),
                "the invite for ada@example.com was handed over before",
            ),
            (
                Ok(Delivery::Withheld),
                "did not email the invite to ada@example.com, since an invite goes only by a mail server to an address on TETSU_MAIL_ALLOW, and is never kept; the link above still works",
            ),
            (
                Err(MailError::Unavailable),
                "could not email the invite link to ada@example.com just now; the link above still works",
            ),
            (
                Err(MailError::Refused),
                "the mail server refused the invite for ada@example.com; the link above still works",
            ),
        ] {
            assert_eq!(delivered(&to, sent), said);
        }
    }

    /// A `.env` names where links point only when it names the database too, so a stray one cannot send a real
    /// database's links to another site.
    #[test]
    fn a_link_points_where_its_database_is_named() {
        let origin = "TETSU_ORIGIN=https://elsewhere.example\n";
        let refused = link_origin(&Local::file_only(origin)).unwrap_err();
        assert!(
            refused.starts_with("TETSU_ORIGIN comes from .env but DATABASE_URL does not"),
            "{refused}"
        );
        let both = format!("{origin}DATABASE_URL=postgres://localhost/tetsu\n");
        assert_eq!(
            link_origin(&Local::file_only(&both)),
            Ok("https://elsewhere.example".into())
        );
    }

    /// Only a small regular file is read, so a pipe or a huge file never holds the command up.
    #[test]
    fn only_a_small_regular_file_is_read() {
        let small = Scratch::file("hostname-small", "workstation\n").unwrap();
        assert_eq!(read_small(&*small), Some("workstation\n".into()));
        let full = Scratch::file("hostname-full", &"a".repeat(4096)).unwrap();
        assert_eq!(read_small(&*full).map(|text| text.len()), Some(4096));
        let large = Scratch::file("hostname-large", &"a".repeat(4097)).unwrap();
        assert_eq!(read_small(&*large), None);
        let folder = Scratch::folder("hostname-folder").unwrap();
        assert_eq!(read_small(&*folder), None);
        assert_eq!(read_small(folder.join("missing")), None);
        let pipe = folder.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap();
        assert!(made.success());
        assert_eq!(read_small(&pipe), None);
        let binary = Scratch::folder("hostname-binary").unwrap();
        std::fs::write(binary.join("hostname"), [0xff, 0xfe]).unwrap();
        assert_eq!(read_small(binary.join("hostname")), None);
    }

    /// Who runs the command is `SUDO_USER`, `USER`, or `LOGNAME`, the first that is set and not blank, and no one
    /// when one is not UTF-8.
    #[test]
    fn the_login_is_the_first_set_of_sudo_user_user_and_logname() {
        let from = |values: [Option<&str>; 3]| {
            login(|name| {
                let at = ["SUDO_USER", "USER", "LOGNAME"]
                    .iter()
                    .position(|known| *known == name)
                    .expect("a login variable");
                match values[at] {
                    Some("\u{FFFD}") => Err(VarError::NotUnicode("\u{FFFD}".into())),
                    Some(value) => Ok(value.to_string()),
                    None => Err(VarError::NotPresent),
                }
            })
        };
        assert_eq!(
            from([Some("kurt"), Some("root"), Some("root")]),
            Some("kurt".into())
        );
        assert_eq!(
            from([None, Some("kurt"), Some("other")]),
            Some("kurt".into())
        );
        assert_eq!(
            from([Some(" "), Some(""), Some("kurt")]),
            Some("kurt".into())
        );
        assert_eq!(from([None, Some("\u{FFFD}"), Some("mallory")]), None);
        assert_eq!(from([None, None, None]), None);
    }

    /// The host name is the first line of `/etc/hostname` that is neither blank nor a comment.
    #[test]
    fn the_host_name_skips_comments_and_blank_lines() {
        assert_eq!(host_name("workstation\n"), Some("workstation"));
        assert_eq!(
            host_name("# managed by ansible\n\n  prod-bastion  \n"),
            Some("prod-bastion")
        );
        assert_eq!(host_name("# only a comment\n"), None);
    }
}
