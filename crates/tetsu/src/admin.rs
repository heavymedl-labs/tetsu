//! The admin command (ADR-0040, ADR-0059): creates admins and issues their invite links, lists them,
//! disables and enables them, and ends their sessions. Identity records each change for the audit record, with
//! who ran the command and what came of it.
//!
//! An invite link is printed only to a terminal, never to a pipe, so it cannot land in CI's output or a log by
//! accident, and emailed only to an address on the allowlist; it is never kept for the mailbox, where another
//! admin could read it. Who can reach a link is whoever runs this command against the database, and the holder
//! of the admin's mailbox (ADR-0040, ADR-0074, `v5.0.0-6.4.1`).

use tetsu_identity::{
    AccountId, Accounts, Actor, AdminStore, AdminSummary, Disabling, InviteToken, PersonName,
    Removal, invite_message,
};
use tetsu_kernel::{Address, Mailer};

use crate::operator::{self, Acting};
use crate::settings::Local;

/// An admin command, with what it acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum Admin {
    Create { name: PersonName, email: Address },
    Email { id: AccountId, email: Address },
    Invite { id: AccountId },
    List,
    Disable { id: AccountId },
    Enable { id: AccountId },
    RemovePasskeys { id: AccountId },
    EndSessions { id: AccountId },
    EndAllSessions,
}

const USAGE: &str = "admin takes create <name> <email>, email <id> <email>, invite <id>, list, disable <id>, \
                     enable <id>, remove-passkeys <id>, or end-sessions <id | --all>";

/// The admin command its words name, with its arguments checked.
pub fn parse(rest: &[String]) -> Result<Admin, String> {
    let Some((command, arguments)) = rest.split_first() else {
        return Err(USAGE.into());
    };
    match (command.as_str(), arguments) {
        ("create", [name, email]) => creating(name, email),
        ("email", [id, email]) => emailing(id, email),
        ("list", []) => Ok(Admin::List),
        ("end-sessions", [all]) if all == "--all" => Ok(Admin::EndAllSessions),
        (command, [text]) => with_id(command, text),
        _ => Err(USAGE.into()),
    }
}

/// Creating an admin named `name`, at `email`. A name like an option, such as --help, is a mistake, never an
/// admin to create and join.
fn creating(name: &str, email: &str) -> Result<Admin, String> {
    if name.trim_start().starts_with('-') {
        return Err(USAGE.into());
    }
    let name = PersonName::parse(name).map_err(|error| error.to_string())?;
    Ok(Admin::Create {
        name,
        email: plain(email)?,
    })
}

/// Setting admin `id`'s address to `email`.
fn emailing(id: &str, email: &str) -> Result<Admin, String> {
    let id = AccountId::parse(id).ok_or(format!("{id:?} is not an admin ID"))?;
    Ok(Admin::Email {
        id,
        email: plain(email)?,
    })
}

fn plain(email: &str) -> Result<Address, String> {
    Address::parse(email).ok_or(format!("{email:?} is not a plain email address"))
}

/// A command that acts on one admin, with that admin's ID.
fn with_id(command: &str, text: &str) -> Result<Admin, String> {
    let named: fn(AccountId) -> Admin = match command {
        "invite" => |id| Admin::Invite { id },
        "disable" => |id| Admin::Disable { id },
        "enable" => |id| Admin::Enable { id },
        "remove-passkeys" => |id| Admin::RemovePasskeys { id },
        "end-sessions" => |id| Admin::EndSessions { id },
        _ => return Err(USAGE.into()),
    };
    AccountId::parse(text)
        .map(named)
        .ok_or(format!("{text:?} is not an admin ID"))
}

/// Runs an admin command with the settings `local` loads, and returns the lines it reports. `terminal` says
/// whether those lines go to a terminal, which a command that issues a link requires, checked before any setting is
/// read.
pub async fn run(
    admin: Admin,
    local: impl FnOnce() -> Result<Local, String>,
    terminal: bool,
) -> Result<Vec<String>, String> {
    let issues_link = matches!(admin, Admin::Create { .. } | Admin::Invite { .. });
    if issues_link && !terminal {
        return Err(operator::TERMINAL_ONLY.into());
    }
    let local = local()?;
    let origin = if issues_link {
        operator::link_origin(&local)?
    } else {
        String::new()
    };
    let by = operator::who();
    // Only a command that issues a link sends mail, so only it reads the mail settings.
    let (smtp, allow) = if issues_link {
        crate::mail(&local)?
    } else {
        (None, tetsu_mail::Allowlist::default())
    };
    let pool = crate::connect(&local).await?;
    let office = tetsu_mail::post_office(pool.clone(), smtp, allow);
    let admins = tetsu_identity::postgres_store(pool);
    act(&admins, &office, admin, &Acting { by, origin }).await
}

/// Does what an admin command asks, through Identity's use cases, emailing any invite link it issues.
async fn act(
    admins: &Accounts<impl AdminStore>,
    mailer: &impl Mailer,
    admin: Admin,
    acting: &Acting,
) -> Result<Vec<String>, String> {
    let (by, origin) = (&acting.by, acting.origin.as_str());
    let said = match admin {
        Admin::Create { name, email } => {
            return created(admins, mailer, &name, &email, acting).await;
        }
        Admin::Email { id, email } => admins
            .set_email(id, &email, by)
            .await
            .map(|done| vec![emailed(id, &email, done)]),
        Admin::Invite { id } => {
            let token = admins
                .invite(id, by)
                .await
                .map_err(|error| error.to_string())?;
            return Ok(issued(admins, mailer, id, origin, &token).await);
        }
        Admin::List => admins
            .list()
            .await
            .map(|all| all.iter().map(listed).collect()),
        Admin::Disable { id } => admins
            .disable(id, by)
            .await
            .map(|done| vec![disabled(id, done)]),
        Admin::Enable { id } => admins
            .enable(id, by)
            .await
            .map(|done| vec![enabled(id, done)]),
        Admin::RemovePasskeys { id } => admins
            .remove_passkeys(id, by)
            .await
            .map(|removal| vec![removed(id, removal)]),
        Admin::EndSessions { id } => admins
            .end_sessions(id, by)
            .await
            .map(|ended| vec![format!("ended {ended} sessions of admin {}", id.get())]),
        Admin::EndAllSessions => admins
            .end_all_sessions(by)
            .await
            .map(|ended| vec![format!("ended {ended} admin sessions")]),
    };
    said.map_err(|error| error.to_string())
}

/// Creates an admin and issues their first invite link's token. These are two steps, so if the second fails the
/// admin exists without a link, and the error names them so the link can be issued again.
async fn create(
    admins: &Accounts<impl AdminStore>,
    name: &PersonName,
    email: &Address,
    by: &Actor,
) -> Result<(AccountId, InviteToken), String> {
    let id = admins
        .create(name, Some(email), by)
        .await
        .map_err(|error| error.to_string())?;
    let token = admins.invite(id, by).await.map_err(|error| {
        format!(
            "created admin {id}, but could not issue their invite link: {error}; run tetsu admin invite {id}",
            id = id.get()
        )
    })?;
    Ok((id, token))
}

/// Creates an admin and emails their first invite link, saying what came of each.
async fn created(
    admins: &Accounts<impl AdminStore>,
    mailer: &impl Mailer,
    name: &PersonName,
    email: &Address,
    acting: &Acting,
) -> Result<Vec<String>, String> {
    let (id, token) = create(admins, name, email, &acting.by).await?;
    let mut said = vec![format!("created admin {}, {}", id.get(), name.as_str())];
    said.extend(issued(admins, mailer, id, &acting.origin, &token).await);
    Ok(said)
}

/// What setting an admin's address did.
fn emailed(id: AccountId, email: &Address, done: bool) -> String {
    if done {
        format!(
            "admin {} now gets their invites and notices at {}",
            id.get(),
            email.as_str()
        )
    } else {
        format!("admin {} already has that address", id.get())
    }
}

/// What enabling an admin did.
fn enabled(id: AccountId, done: bool) -> String {
    if done {
        format!("enabled admin {}, who logs in afresh", id.get())
    } else {
        format!("admin {} was already enabled", id.get())
    }
}

/// What issuing a link said: the link, printed as always, and what came of emailing it (ADR-0074). A link that
/// could not be emailed still works, so mail never fails the command.
async fn issued(
    admins: &Accounts<impl AdminStore>,
    mailer: &impl Mailer,
    id: AccountId,
    origin: &str,
    token: &InviteToken,
) -> Vec<String> {
    let url = operator::link_url(origin, "/admin/join", token);
    let emailed = match admins.email(id).await {
        Ok(Some(to)) => match invite_message(to.clone(), &url, token) {
            Some(message) => operator::delivered(&to, mailer.send(&message).await),
            None => {
                format!(
                    "could not write the invite email for {}; the link above still works",
                    to.as_str()
                )
            }
        },
        Ok(None) => format!(
            "admin {} has no email address, so the link was not emailed",
            id.get()
        ),
        Err(error) => format!("could not read admin {}'s email address: {error}", id.get()),
    };
    vec![
        format!("invite link, good once and for an hour: {url}"),
        emailed,
    ]
}

/// What removing an admin's passkeys did, and what they do next.
fn removed(id: AccountId, removal: Removal) -> String {
    format!(
        "removed {} passkeys of admin {id}, ending {} sessions; issue a link with tetsu admin invite {id}",
        removal.passkeys_removed,
        removal.sessions_ended,
        id = id.get()
    )
}

/// What disabling an admin did, with how many sessions and invite links it ended.
fn disabled(id: AccountId, done: Disabling) -> String {
    if done.done {
        format!(
            "disabled admin {}, ending {} sessions and {} invite links",
            id.get(),
            done.sessions_ended,
            done.links_ended
        )
    } else {
        format!("admin {} was already disabled", id.get())
    }
}

/// One admin as `list` shows them: ID, name, and whether they are enabled, two spaces apart, since the
/// command's output escapes tabs with every other control character.
fn listed(admin: &AdminSummary) -> String {
    let state = if admin.disabled_at.is_some() {
        "disabled"
    } else {
        "enabled"
    };
    let passkeys = match admin.passkeys {
        1 => "1 passkey".to_string(),
        count => format!("{count} passkeys"),
    };
    let link = admin
        .link_minutes
        .map(|minutes| format!("  link open {minutes} min"))
        .unwrap_or_default();
    format!(
        "{}  {}  {state}  {passkeys}{link}",
        admin.id.get(),
        admin.name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args;
    use tetsu_kernel::Mailbox;
    use tetsu_mail::PostOffice;

    /// A name like an option, such as --help, is a mistake, never an admin to create and join.
    #[test]
    fn a_name_like_an_option_is_never_created() {
        for name in ["--help", "-x", " --help", "\u{A0}-x"] {
            assert_eq!(
                parse(&args(&["create", name, "a@example.com"])),
                Err(USAGE.into()),
                "{name:?}"
            );
        }
    }

    /// Setting an address takes an admin ID and a plain address, and nothing more.
    #[test]
    fn the_email_command_takes_an_id_and_a_plain_address() {
        assert_eq!(
            parse(&args(&["email", "7", "kurt@example.com"])),
            Ok(Admin::Email {
                id: AccountId::parse("7").unwrap(),
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
            Err("\"seven\" is not an admin ID".into())
        );
        assert_eq!(
            parse(&args(&["email", "7", "Kurt <kurt@example.com>"])),
            Err("\"Kurt <kurt@example.com>\" is not a plain email address".into())
        );
    }

    #[test]
    fn admin_commands_are_parsed_exactly() {
        let kurt = AccountId::parse("7").unwrap();
        for (words, parsed) in [
            (
                &["create", "Kurt Medley", "kurt@example.com"][..],
                Admin::Create {
                    name: PersonName::parse("Kurt Medley").unwrap(),
                    email: Address::parse("kurt@example.com").unwrap(),
                },
            ),
            (&["invite", "7"], Admin::Invite { id: kurt }),
            (&["list"], Admin::List),
            (&["disable", "7"], Admin::Disable { id: kurt }),
            (&["enable", "7"], Admin::Enable { id: kurt }),
            (
                &["remove-passkeys", "7"],
                Admin::RemovePasskeys { id: kurt },
            ),
            (&["end-sessions", "7"], Admin::EndSessions { id: kurt }),
            (&["end-sessions", "--all"], Admin::EndAllSessions),
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
        ] {
            assert_eq!(parse(&args(bad)), Err(USAGE.into()), "{bad:?}");
        }
        assert_eq!(
            parse(&args(&["disable", "seven"])),
            Err("\"seven\" is not an admin ID".into())
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
        for admin in [
            creating("Kurt Medley"),
            Admin::Invite {
                id: AccountId::parse("7").unwrap(),
            },
        ] {
            let refused = run(admin, || panic!("reads no settings"), false)
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
            Admin::Invite {
                id: AccountId::parse("7").unwrap(),
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
            "DATABASE_URL=postgres://127.0.0.1:1/none\nTETSU_ORIGIN=https://admin.example.com\n";
        let invite = || Admin::Invite {
            id: AccountId::parse("7").unwrap(),
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
                .beside("DATABASE_URL", &crate::settings::MAIL)
                .unwrap_err()
                .starts_with("TETSU_SMTP_HOST comes from .env but DATABASE_URL does not")
        );
    }

    /// Creating an admin issues their invite and prints it; with no mail server set, the invite is withheld
    /// rather than kept, since the mailbox would show any admin its live link.
    #[tokio::test]
    async fn a_created_admins_invite_is_never_kept() {
        let (admins, office, _database) = migrated().await;
        let created = said(&admins, &office, creating("Kurt Medley")).await;
        assert_eq!(created.len(), 3, "{created:?}");
        assert_eq!(created[0], "created admin 1, Kurt Medley");
        assert!(created[1].starts_with(
            "invite link, good once and for an hour: https://admin.example.com/admin/join#"
        ));
        assert!(created[2].starts_with("did not email the invite to kurt@example.com, since"));
        assert_eq!(office.captured().await.unwrap(), vec![]);
    }

    /// An admin with no address is still issued a link, which is printed and not emailed; one whose address
    /// cannot be read says so.
    #[tokio::test]
    async fn an_invite_without_an_address_is_printed_only() {
        let (admins, office, database) = migrated().await;
        let id = admins
            .create(&PersonName::parse("Lee Park").unwrap(), None, &by())
            .await
            .unwrap();
        let invited = said(&admins, &office, Admin::Invite { id }).await;
        assert_eq!(
            invited[1],
            "admin 1 has no email address, so the link was not emailed"
        );
        tetsu_postgres::connect(&database)
            .await
            .unwrap()
            .batch_execute("REVOKE SELECT (email) ON identity.admins FROM tetsu_identity")
            .await
            .unwrap();
        let invited = said(&admins, &office, Admin::Invite { id }).await;
        assert!(
            invited[1].starts_with("could not read admin 1's email address: "),
            "{invited:?}"
        );
        assert_eq!(office.captured().await.unwrap(), vec![]);
    }

    /// A list line counts passkeys in words that read right for one, and shows a link only while it is open.
    #[test]
    fn a_list_line_counts_passkeys_and_shows_an_open_link() {
        let admin = |passkeys, link_minutes| AdminSummary {
            id: AccountId::parse("3").unwrap(),
            name: "Ana Ortiz".into(),
            created_at: std::time::SystemTime::UNIX_EPOCH,
            disabled_at: None,
            passkeys,
            link_minutes,
        };
        assert_eq!(listed(&admin(1, None)), "3  Ana Ortiz  enabled  1 passkey");
        assert_eq!(
            listed(&admin(2, Some(7))),
            "3  Ana Ortiz  enabled  2 passkeys  link open 7 min"
        );
    }

    const ORIGIN: &str = "https://admin.example.com";

    /// Creating `name`, whose address is their first name at example.com.
    fn creating(name: &str) -> Admin {
        let first = name.split(' ').next().unwrap_or(name).to_lowercase();
        Admin::Create {
            name: PersonName::parse(name).unwrap(),
            email: Address::parse(&format!("{first}@example.com")).unwrap(),
        }
    }

    fn by() -> Actor {
        Actor::parse("kurt@workstation").unwrap()
    }

    fn acting() -> Acting {
        Acting {
            by: by(),
            origin: ORIGIN.to_string(),
        }
    }

    /// What a command said, as the maintainer ran it.
    async fn said(
        admins: &Accounts<impl AdminStore>,
        office: &PostOffice,
        admin: Admin,
    ) -> Vec<String> {
        act(admins, office, admin, &acting()).await.unwrap()
    }

    /// Identity's use cases and a post office that keeps every message, over a fresh, migrated database, kept while
    /// the test runs.
    async fn migrated() -> (
        Accounts<impl AdminStore>,
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
        let (admins, office, _database) = migrated().await;
        let kurt = AccountId::parse("1").unwrap();
        let created = said(&admins, &office, creating("Kurt Medley")).await;
        assert_eq!(created[0], "created admin 1, Kurt Medley");
        let prefix =
            "invite link, good once and for an hour: https://admin.example.com/admin/join#";
        for line in [
            &created[1],
            &said(&admins, &office, Admin::Invite { id: kurt }).await[0],
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
            said(&admins, &office, Admin::List).await,
            ["1  Kurt Medley  enabled  0 passkeys  link open 60 min"]
        );
        assert_eq!(
            said(&admins, &office, Admin::EndSessions { id: kurt }).await,
            ["ended 0 sessions of admin 1"]
        );
        assert_eq!(
            said(&admins, &office, Admin::EndAllSessions).await,
            ["ended 0 admin sessions"]
        );
        let email = |text: &str| Admin::Email {
            id: kurt,
            email: Address::parse(text).unwrap(),
        };
        assert_eq!(
            said(&admins, &office, email("kurt@example.org")).await,
            ["admin 1 now gets their invites and notices at kurt@example.org"]
        );
        assert_eq!(
            said(&admins, &office, email("kurt@example.org")).await,
            ["admin 1 already has that address"]
        );
    }

    /// Removing an admin's passkeys says how many and what ended, and names the command that issues a new link.
    #[tokio::test]
    async fn removing_passkeys_names_the_next_step() {
        let (admins, office, _database) = migrated().await;
        said(&admins, &office, creating("Kurt Medley")).await;
        assert_eq!(
            said(
                &admins,
                &office,
                Admin::RemovePasskeys {
                    id: AccountId::parse("1").unwrap()
                }
            )
            .await,
            [
                "removed 0 passkeys of admin 1, ending 0 sessions; issue a link with tetsu admin invite 1"
            ]
        );
    }

    /// Disabling and enabling say what they did, and a disabled admin is issued no link.
    #[tokio::test]
    async fn an_admin_is_disabled_and_enabled_again() {
        let (admins, office, _database) = migrated().await;
        let kurt = AccountId::parse("1").unwrap();
        said(&admins, &office, creating("Kurt Medley")).await;
        assert_eq!(
            said(&admins, &office, Admin::Disable { id: kurt }).await,
            ["disabled admin 1, ending 0 sessions and 1 invite links"]
        );
        assert_eq!(
            said(&admins, &office, Admin::Disable { id: kurt }).await,
            ["admin 1 was already disabled"]
        );
        assert_eq!(
            said(&admins, &office, Admin::List).await,
            ["1  Kurt Medley  disabled  0 passkeys"]
        );
        assert_eq!(
            act(&admins, &office, Admin::Invite { id: kurt }, &acting()).await,
            Err("invalid: no enabled admin 1".into())
        );
        assert_eq!(
            said(&admins, &office, Admin::Enable { id: kurt }).await,
            ["enabled admin 1, who logs in afresh"]
        );
        assert_eq!(
            said(&admins, &office, Admin::Enable { id: kurt }).await,
            ["admin 1 was already enabled"]
        );

        let nobody = AccountId::parse("9").unwrap();
        for command in [
            Admin::Disable { id: nobody },
            Admin::Enable { id: nobody },
            Admin::RemovePasskeys { id: nobody },
            Admin::EndSessions { id: nobody },
        ] {
            assert_eq!(
                act(&admins, &office, command, &acting()).await,
                Err("invalid: no admin 9".into())
            );
        }
    }

    /// When the link cannot be issued after the admin is made, the error names the admin, so a link can be
    /// issued again rather than a second admin made.
    #[tokio::test]
    async fn a_failed_link_after_creating_names_the_new_admin() {
        let (admins, office, database) = migrated().await;
        tetsu_postgres::connect(&database)
            .await
            .unwrap()
            .batch_execute(
                "REVOKE EXECUTE ON FUNCTION identity.issue_invite(bytea, bigint) FROM tetsu_identity",
            )
            .await
            .unwrap();
        let created = creating("Kurt Medley");
        let refused = act(&admins, &office, created, &acting()).await.unwrap_err();
        assert!(
            refused.starts_with("created admin 1, but could not issue their invite link: ")
                && refused.ends_with("; run tetsu admin invite 1"),
            "{refused}"
        );
    }
}
