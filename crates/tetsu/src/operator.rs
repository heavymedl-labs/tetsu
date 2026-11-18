//! The operator command (ADR-0040, ADR-0059): creates operators and issues their invite links, lists them,
//! disables and enables them, and ends their sessions. Identity records each change for the audit record, with
//! who ran the command and what came of it.
//!
//! An invite link is printed only to a terminal, never to a pipe, so it cannot land in CI's output or a log by
//! accident, and emailed only to an address on the allowlist; it is never kept for the mailbox, where another
//! operator could read it. Who can reach a link is whoever runs this command against the database, and the holder
//! of the operator's mailbox (ADR-0040, ADR-0074, `v5.0.0-6.4.1`).

use std::env::VarError;
use std::io::Read;

use tetsu_identity::{
    Actor, Disabling, InviteToken, OperatorId, OperatorName, OperatorStore, OperatorSummary,
    Operators, Removal, invite_message,
};
use tetsu_kernel::{Address, Delivery, MailError, Mailer};

use crate::settings::{self, Local};

/// An operator command, with what it acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum Operator {
    Create { name: OperatorName, email: Address },
    Email { id: OperatorId, email: Address },
    Invite { id: OperatorId },
    List,
    Disable { id: OperatorId },
    Enable { id: OperatorId },
    RemovePasskeys { id: OperatorId },
    EndSessions { id: OperatorId },
    EndAllSessions,
}

const USAGE: &str = "operator takes create <name> <email>, email <id> <email>, invite <id>, list, disable <id>, \
                     enable <id>, remove-passkeys <id>, or end-sessions <id | --all>";

/// The operator command its words name, with its arguments checked.
pub fn parse(rest: &[String]) -> Result<Operator, String> {
    let Some((command, arguments)) = rest.split_first() else {
        return Err(USAGE.into());
    };
    match (command.as_str(), arguments) {
        // A name like an option, such as --help, is a mistake, never an operator to create and join.
        ("create", [name, _]) if name.trim_start().starts_with('-') => Err(USAGE.into()),
        ("create", [name, email]) => {
            let name = OperatorName::parse(name).map_err(|error| error.to_string())?;
            Ok(Operator::Create {
                name,
                email: plain(email)?,
            })
        }
        ("email", [id, email]) => {
            let id = OperatorId::parse(id).ok_or(format!("{id:?} is not an operator ID"))?;
            Ok(Operator::Email {
                id,
                email: plain(email)?,
            })
        }
        ("list", []) => Ok(Operator::List),
        ("end-sessions", [all]) if all == "--all" => Ok(Operator::EndAllSessions),
        (command, [text]) => with_id(command, text),
        _ => Err(USAGE.into()),
    }
}

fn plain(email: &str) -> Result<Address, String> {
    Address::parse(email).ok_or(format!("{email:?} is not a plain email address"))
}

/// A command that acts on one operator, with that operator's ID.
fn with_id(command: &str, text: &str) -> Result<Operator, String> {
    let named: fn(OperatorId) -> Operator = match command {
        "invite" => |id| Operator::Invite { id },
        "disable" => |id| Operator::Disable { id },
        "enable" => |id| Operator::Enable { id },
        "remove-passkeys" => |id| Operator::RemovePasskeys { id },
        "end-sessions" => |id| Operator::EndSessions { id },
        _ => return Err(USAGE.into()),
    };
    OperatorId::parse(text)
        .map(named)
        .ok_or(format!("{text:?} is not an operator ID"))
}

/// Runs an operator command with the settings `local` loads, and returns the lines it reports. `terminal` says
/// whether those lines go to a terminal, which a command that issues a link requires, checked before any setting is
/// read.
pub async fn run(
    operator: Operator,
    local: impl FnOnce() -> Result<Local, String>,
    terminal: bool,
) -> Result<Vec<String>, String> {
    let issues_link = matches!(operator, Operator::Create { .. } | Operator::Invite { .. });
    if issues_link && !terminal {
        return Err(
            "an invite link prints only to a terminal, never to a log or a pipe (ADR-0040); run this in one"
                .into(),
        );
    }
    let local = local()?;
    let origin = if issues_link {
        link_origin(&local)?
    } else {
        String::new()
    };
    let by = Actor::from_login(
        login(|name| std::env::var(name)).as_deref(),
        read_small("/etc/hostname").as_deref().and_then(host_name),
    );
    // Only a command that issues a link sends mail, so only it reads the mail settings.
    let mail = if issues_link {
        local.beside("DATABASE_URL", &settings::MAIL)?;
        settings::mail(|name| local.var(name))?
    } else {
        settings::Mail {
            smtp: None,
            allow: tetsu_mail::Allowlist::default(),
        }
    };
    let smtp = settings::smtp(&mail)?;
    let pool = crate::connect(&local).await?;
    let office = tetsu_mail::post_office(pool.clone(), smtp, mail.allow);
    let operators = tetsu_identity::postgres_store(pool);
    act(&operators, &office, operator, &origin, &by).await
}

/// Where a link points. A link holds a live token, so a `.env` may name the origin only when it names the database
/// too, and never point a real database's links at another site.
fn link_origin(local: &Local) -> Result<String, String> {
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

/// Does what an operator command asks, through Identity's use cases, emailing any invite link it issues.
async fn act(
    operators: &Operators<impl OperatorStore>,
    mailer: &impl Mailer,
    operator: Operator,
    origin: &str,
    by: &Actor,
) -> Result<Vec<String>, String> {
    let said = match operator {
        Operator::Create { name, email } => {
            return create(operators, mailer, &name, &email, origin, by).await;
        }
        Operator::Email { id, email } => operators.set_email(id, &email, by).await.map(|done| {
            vec![if done {
                format!(
                    "operator {} now gets their invites and notices at {}",
                    id.get(),
                    email.as_str()
                )
            } else {
                format!("operator {} already has that address", id.get())
            }]
        }),
        Operator::Invite { id } => match operators.invite(id, by).await {
            Ok(token) => return Ok(issued(operators, mailer, id, origin, &token).await),
            Err(error) => Err(error),
        },
        Operator::List => operators
            .list()
            .await
            .map(|all| all.iter().map(listed).collect()),
        Operator::Disable { id } => operators
            .disable(id, by)
            .await
            .map(|done| vec![disabled(id, done)]),
        Operator::Enable { id } => operators.enable(id, by).await.map(|done| {
            vec![if done {
                format!("enabled operator {}, who logs in afresh", id.get())
            } else {
                format!("operator {} was already enabled", id.get())
            }]
        }),
        Operator::RemovePasskeys { id } => operators
            .remove_passkeys(id, by)
            .await
            .map(|removal| vec![removed(id, removal)]),
        Operator::EndSessions { id } => operators
            .end_sessions(id, by)
            .await
            .map(|ended| vec![format!("ended {ended} sessions of operator {}", id.get())]),
        Operator::EndAllSessions => operators
            .end_all_sessions(by)
            .await
            .map(|ended| vec![format!("ended {ended} operator sessions")]),
    };
    said.map_err(|error| error.to_string())
}

/// Creates an operator and issues their first invite link. These are two steps, so if the second fails the
/// operator exists without a link, and the error names them so the link can be issued again.
async fn create(
    operators: &Operators<impl OperatorStore>,
    mailer: &impl Mailer,
    name: &OperatorName,
    email: &Address,
    origin: &str,
    by: &Actor,
) -> Result<Vec<String>, String> {
    let id = operators
        .create(name, Some(email), by)
        .await
        .map_err(|error| error.to_string())?;
    let token = operators.invite(id, by).await.map_err(|error| {
        format!(
            "created operator {id}, but could not issue their invite link: {error}; run tetsu operator invite {id}",
            id = id.get()
        )
    })?;
    let mut said = vec![format!("created operator {}, {}", id.get(), name.as_str())];
    said.extend(issued(operators, mailer, id, origin, &token).await);
    Ok(said)
}

/// What issuing a link said: the link, printed as always, and what came of emailing it (ADR-0074). A link that
/// could not be emailed still works, so mail never fails the command.
async fn issued(
    operators: &Operators<impl OperatorStore>,
    mailer: &impl Mailer,
    id: OperatorId,
    origin: &str,
    token: &InviteToken,
) -> Vec<String> {
    let url = link_url(origin, token);
    let emailed = match operators.email(id).await {
        Ok(Some(to)) => match invite_message(to.clone(), &url, token) {
            Some(message) => delivered(&to, mailer.send(&message).await),
            None => {
                format!(
                    "could not write the invite email for {}; the link above still works",
                    to.as_str()
                )
            }
        },
        Ok(None) => format!(
            "operator {} has no email address, so the link was not emailed",
            id.get()
        ),
        Err(error) => format!(
            "could not read operator {}'s email address: {error}",
            id.get()
        ),
    };
    vec![
        format!("invite link, good once and for an hour: {url}"),
        emailed,
    ]
}

/// What came of emailing an invite to `to`.
fn delivered(to: &Address, sent: Result<Delivery, MailError>) -> String {
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
            "kept the invite for {to} in the mailbox, which should never happen; disable the operator and report it"
        ),
        Err(MailError::Unavailable) => {
            format!("could not email the invite link to {to} just now; {still}")
        }
        Err(MailError::Refused) => format!("the mail server refused the invite for {to}; {still}"),
    }
}

/// What removing an operator's passkeys did, and what they do next.
fn removed(id: OperatorId, removal: Removal) -> String {
    format!(
        "removed {} passkeys of operator {id}, ending {} sessions; issue a link with tetsu operator invite {id}",
        removal.passkeys_removed,
        removal.sessions_ended,
        id = id.get()
    )
}

/// What disabling an operator did, with how many sessions and invite links it ended.
fn disabled(id: OperatorId, done: Disabling) -> String {
    if done.done {
        format!(
            "disabled operator {}, ending {} sessions and {} invite links",
            id.get(),
            done.sessions_ended,
            done.links_ended
        )
    } else {
        format!("operator {} was already disabled", id.get())
    }
}

/// One operator as `list` shows them: ID, name, and whether they are enabled, two spaces apart, since the
/// command's output escapes tabs with every other control character.
fn listed(operator: &OperatorSummary) -> String {
    let state = if operator.disabled_at.is_some() {
        "disabled"
    } else {
        "enabled"
    };
    let passkeys = match operator.passkeys {
        1 => "1 passkey".to_string(),
        count => format!("{count} passkeys"),
    };
    let link = operator
        .link_minutes
        .map(|minutes| format!("  link open {minutes} min"))
        .unwrap_or_default();
    format!(
        "{}  {}  {state}  {passkeys}{link}",
        operator.id.get(),
        operator.name
    )
}

/// The invite link, with the token in the fragment, which a browser never sends to the server or in a
/// `Referer`, so it stays out of the server's logs. A browser's history keeps it, and a container run with a
/// terminal (`docker run -t`) logs it, which is why it is good once and for an hour.
fn link_url(origin: &str, token: &InviteToken) -> String {
    format!("{origin}/ops/join#{}", token.to_text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args;
    use tetsu_kernel::Mailbox;
    use tetsu_mail::PostOffice;
    use tetsu_postgres::test_support::Scratch;

    /// Setting an address takes an operator ID and a plain address, and nothing more.
    #[test]
    fn the_email_command_takes_an_id_and_a_plain_address() {
        assert_eq!(
            parse(&args(&["email", "7", "kurt@example.com"])),
            Ok(Operator::Email {
                id: OperatorId::parse("7").unwrap(),
                email: Address::parse("kurt@example.com").unwrap(),
            })
        );
        for bad in [
            &["email", "7"][..],
            &["email", "7", "kurt@example.com", "more"],
        ] {
            assert_eq!(parse(&args(bad)), Err(USAGE.into()), "{bad:?}");
        }
        assert_eq!(
            parse(&args(&["email", "seven", "kurt@example.com"])),
            Err("\"seven\" is not an operator ID".into())
        );
        assert_eq!(
            parse(&args(&["email", "7", "Kurt <kurt@example.com>"])),
            Err("\"Kurt <kurt@example.com>\" is not a plain email address".into())
        );
    }

    #[test]
    fn operator_commands_are_parsed_exactly() {
        let kurt = OperatorId::parse("7").unwrap();
        for (words, parsed) in [
            (
                &["create", "Kurt Medley", "kurt@example.com"][..],
                Operator::Create {
                    name: OperatorName::parse("Kurt Medley").unwrap(),
                    email: Address::parse("kurt@example.com").unwrap(),
                },
            ),
            (&["invite", "7"], Operator::Invite { id: kurt }),
            (&["list"], Operator::List),
            (&["disable", "7"], Operator::Disable { id: kurt }),
            (&["enable", "7"], Operator::Enable { id: kurt }),
            (
                &["remove-passkeys", "7"],
                Operator::RemovePasskeys { id: kurt },
            ),
            (&["end-sessions", "7"], Operator::EndSessions { id: kurt }),
            (&["end-sessions", "--all"], Operator::EndAllSessions),
        ] {
            assert_eq!(parse(&args(words)), Ok(parsed), "{words:?}");
        }
        for bad in [
            &[][..],
            &["create"],
            &["create", "Kurt Medley"],
            &["create", "Kurt Medley", "kurt@example.com", "more"],
            &["list", "all"],
            &["remove", "7"],
            &["end-sessions"],
            &["create", "--help", "a@example.com"],
            &["create", "-x", "a@example.com"],
            &["create", " --help", "a@example.com"],
            &["create", "\u{A0}-x", "a@example.com"],
        ] {
            assert_eq!(parse(&args(bad)), Err(USAGE.into()), "{bad:?}");
        }
        assert_eq!(
            parse(&args(&["disable", "seven"])),
            Err("\"seven\" is not an operator ID".into())
        );
        assert!(
            parse(&args(&["create", "", "a@example.com"])).is_err(),
            "an empty name"
        );
        assert_eq!(
            parse(&args(&["create", "Kurt Medley", "Kurt <kurt@example.com>"])),
            Err("\"Kurt <kurt@example.com>\" is not a plain email address".into())
        );
    }

    /// A link is issued only to a terminal, checked before anything is created or read.
    #[tokio::test]
    async fn a_link_is_issued_only_to_a_terminal() {
        for operator in [
            creating("Kurt Medley"),
            Operator::Invite {
                id: OperatorId::parse("7").unwrap(),
            },
        ] {
            let refused = run(operator, || panic!("reads no settings"), false)
                .await
                .unwrap_err();
            assert!(
                refused.starts_with("an invite link prints only to a terminal"),
                "{refused}"
            );
        }
    }

    /// Without the site's origin there is nowhere for a link to point, so nothing is created.
    #[tokio::test]
    async fn a_link_needs_the_sites_origin() {
        let refused = run(
            Operator::Invite {
                id: OperatorId::parse("7").unwrap(),
            },
            || Ok(Local::default()),
            true,
        )
        .await;
        assert_eq!(refused, Err("TETSU_ORIGIN must be set".into()));
    }

    /// Mail settings that are wrong stop a command that issues a link before it touches the database, and a
    /// stray `.env` cannot reroute a named database's mail.
    #[tokio::test]
    async fn wrong_mail_settings_stop_a_link_before_anything_is_made() {
        let base =
            "DATABASE_URL=postgres://127.0.0.1:1/none\nTETSU_ORIGIN=https://ops.example.com\n";
        let invite = || Operator::Invite {
            id: OperatorId::parse("7").unwrap(),
        };
        let missing = format!("{base}TETSU_SMTP_HOST=smtp.example.com\n");
        assert_eq!(
            run(invite(), || Ok(Local::file_only(&missing)), true).await,
            Err("TETSU_MAIL_FROM must be set with TETSU_SMTP_HOST".into())
        );
        let host = format!(
            "{base}TETSU_SMTP_HOST=not a host\nTETSU_SMTP_USERNAME=u\nTETSU_SMTP_PASSWORD=p\n\
             TETSU_MAIL_FROM=tetsu@example.com\n"
        );
        assert_eq!(
            run(invite(), || Ok(Local::file_only(&host)), true).await,
            Err("TETSU_SMTP_HOST must be a host name TLS can check".into())
        );
        let stray = Local::file_only("TETSU_SMTP_HOST=smtp.example.com\n");
        assert!(
            stray
                .beside("DATABASE_URL", &settings::MAIL)
                .unwrap_err()
                .starts_with("TETSU_SMTP_HOST comes from .env but DATABASE_URL does not")
        );
    }

    /// Creating an operator issues their invite and prints it; with no mail server set, the invite is withheld
    /// rather than kept, since the mailbox would show any operator its live link.
    #[tokio::test]
    async fn a_created_operators_invite_is_never_kept() {
        let (operators, office, _database) = migrated().await;
        let created = said(&operators, &office, creating("Kurt Medley")).await;
        assert_eq!(created.len(), 3, "{created:?}");
        assert_eq!(created[0], "created operator 1, Kurt Medley");
        assert!(created[1].starts_with(
            "invite link, good once and for an hour: https://ops.example.com/ops/join#"
        ));
        assert!(created[2].starts_with("did not email the invite to kurt@example.com, since"));
        assert_eq!(office.captured().await.unwrap(), vec![]);
    }

    /// An operator with no address is still issued a link, which is printed and not emailed; one whose address
    /// cannot be read says so.
    #[tokio::test]
    async fn an_invite_without_an_address_is_printed_only() {
        let (operators, office, database) = migrated().await;
        let id = operators
            .create(&OperatorName::parse("Lee Park").unwrap(), None, &by())
            .await
            .unwrap();
        let invited = said(&operators, &office, Operator::Invite { id }).await;
        assert_eq!(
            invited[1],
            "operator 1 has no email address, so the link was not emailed"
        );
        tetsu_postgres::connect(&database)
            .await
            .unwrap()
            .batch_execute("REVOKE SELECT (email) ON identity.operators FROM tetsu_identity")
            .await
            .unwrap();
        let invited = said(&operators, &office, Operator::Invite { id }).await;
        assert!(
            invited[1].starts_with("could not read operator 1's email address: "),
            "{invited:?}"
        );
        assert_eq!(office.captured().await.unwrap(), vec![]);
    }

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
                "kept the invite for ada@example.com in the mailbox, which should never happen; disable the operator and report it",
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

    /// A list line counts passkeys in words that read right for one, and shows a link only while it is open.
    #[test]
    fn a_list_line_counts_passkeys_and_shows_an_open_link() {
        let operator = |passkeys, link_minutes| OperatorSummary {
            id: OperatorId::parse("3").unwrap(),
            name: "Ana Ortiz".into(),
            created_at: std::time::SystemTime::UNIX_EPOCH,
            disabled_at: None,
            passkeys,
            link_minutes,
        };
        assert_eq!(
            listed(&operator(1, None)),
            "3  Ana Ortiz  enabled  1 passkey"
        );
        assert_eq!(
            listed(&operator(2, Some(7))),
            "3  Ana Ortiz  enabled  2 passkeys  link open 7 min"
        );
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

    const ORIGIN: &str = "https://ops.example.com";

    /// Creating `name`, whose address is their first name at example.com.
    fn creating(name: &str) -> Operator {
        let first = name.split(' ').next().unwrap_or(name).to_lowercase();
        Operator::Create {
            name: OperatorName::parse(name).unwrap(),
            email: Address::parse(&format!("{first}@example.com")).unwrap(),
        }
    }

    fn by() -> Actor {
        Actor::parse("kurt@workstation").unwrap()
    }

    /// What a command said, as the maintainer ran it.
    async fn said(
        operators: &Operators<impl OperatorStore>,
        office: &PostOffice,
        operator: Operator,
    ) -> Vec<String> {
        act(operators, office, operator, ORIGIN, &by())
            .await
            .unwrap()
    }

    /// Identity's use cases and a post office that keeps every message, over a fresh, migrated database, kept while
    /// the test runs.
    async fn migrated() -> (
        Operators<impl OperatorStore>,
        PostOffice,
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
        (tetsu_identity::postgres_store(pool), office, database)
    }

    /// Each command acts through Identity's use cases, against a real database, and says what it did.
    #[tokio::test]
    async fn each_command_acts_and_says_what_it_did() {
        let (operators, office, _database) = migrated().await;
        let kurt = OperatorId::parse("1").unwrap();
        let created = said(&operators, &office, creating("Kurt Medley")).await;
        assert_eq!(created[0], "created operator 1, Kurt Medley");
        let prefix = "invite link, good once and for an hour: https://ops.example.com/ops/join#";
        for line in [
            &created[1],
            &said(&operators, &office, Operator::Invite { id: kurt }).await[0],
        ] {
            let token = line
                .strip_prefix(prefix)
                .unwrap_or_else(|| panic!("{line}"));
            assert!(
                token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()),
                "{token}"
            );
        }
        assert_eq!(
            said(&operators, &office, Operator::List).await,
            ["1  Kurt Medley  enabled  0 passkeys  link open 60 min"]
        );
        assert_eq!(
            said(&operators, &office, Operator::EndSessions { id: kurt }).await,
            ["ended 0 sessions of operator 1"]
        );
        assert_eq!(
            said(&operators, &office, Operator::EndAllSessions).await,
            ["ended 0 operator sessions"]
        );
        let email = |text: &str| Operator::Email {
            id: kurt,
            email: Address::parse(text).unwrap(),
        };
        assert_eq!(
            said(&operators, &office, email("kurt@example.org")).await,
            ["operator 1 now gets their invites and notices at kurt@example.org"]
        );
        assert_eq!(
            said(&operators, &office, email("kurt@example.org")).await,
            ["operator 1 already has that address"]
        );
    }

    /// Removing an operator's passkeys says how many and what ended, and names the command that issues a new link.
    #[tokio::test]
    async fn removing_passkeys_names_the_next_step() {
        let (operators, office, _database) = migrated().await;
        said(&operators, &office, creating("Kurt Medley")).await;
        assert_eq!(
            said(
                &operators,
                &office,
                Operator::RemovePasskeys {
                    id: OperatorId::parse("1").unwrap()
                }
            )
            .await,
            [
                "removed 0 passkeys of operator 1, ending 0 sessions; issue a link with tetsu operator invite 1"
            ]
        );
    }

    /// Disabling and enabling say what they did, and a disabled operator is issued no link.
    #[tokio::test]
    async fn an_operator_is_disabled_and_enabled_again() {
        let (operators, office, _database) = migrated().await;
        let kurt = OperatorId::parse("1").unwrap();
        said(&operators, &office, creating("Kurt Medley")).await;
        assert_eq!(
            said(&operators, &office, Operator::Disable { id: kurt }).await,
            ["disabled operator 1, ending 0 sessions and 1 invite links"]
        );
        assert_eq!(
            said(&operators, &office, Operator::Disable { id: kurt }).await,
            ["operator 1 was already disabled"]
        );
        assert_eq!(
            said(&operators, &office, Operator::List).await,
            ["1  Kurt Medley  disabled  0 passkeys"]
        );
        assert_eq!(
            act(
                &operators,
                &office,
                Operator::Invite { id: kurt },
                ORIGIN,
                &by()
            )
            .await,
            Err("invalid: no enabled operator 1".into())
        );
        assert_eq!(
            said(&operators, &office, Operator::Enable { id: kurt }).await,
            ["enabled operator 1, who logs in afresh"]
        );
        assert_eq!(
            said(&operators, &office, Operator::Enable { id: kurt }).await,
            ["operator 1 was already enabled"]
        );

        let nobody = OperatorId::parse("9").unwrap();
        for command in [
            Operator::Disable { id: nobody },
            Operator::Enable { id: nobody },
            Operator::RemovePasskeys { id: nobody },
            Operator::EndSessions { id: nobody },
        ] {
            assert_eq!(
                act(&operators, &office, command, ORIGIN, &by()).await,
                Err("invalid: no operator 9".into())
            );
        }
    }

    /// When the link cannot be issued after the operator is made, the error names the operator, so a link can be
    /// issued again rather than a second operator made.
    #[tokio::test]
    async fn a_failed_link_after_creating_names_the_new_operator() {
        let (operators, office, database) = migrated().await;
        tetsu_postgres::connect(&database)
            .await
            .unwrap()
            .batch_execute(
                "REVOKE EXECUTE ON FUNCTION identity.issue_invite(bytea, bigint) FROM tetsu_identity",
            )
            .await
            .unwrap();
        let by = Actor::parse("kurt@workstation").unwrap();
        let created = creating("Kurt Medley");
        let refused = act(&operators, &office, created, "https://ops.example.com", &by)
            .await
            .unwrap_err();
        assert!(
            refused.starts_with("created operator 1, but could not issue their invite link: ")
                && refused.ends_with("; run tetsu operator invite 1"),
            "{refused}"
        );
    }
}
