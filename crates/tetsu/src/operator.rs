//! The operator command (ADR-0040, ADR-0059): creates operators and issues their enrollment links, lists them,
//! disables and enables them, and ends their sessions. Identity records each change for the audit record, with
//! who ran the command and what came of it.
//!
//! An enrollment link is printed only to a terminal, never to a pipe, so it cannot land in CI's output or a log by
//! accident. That guards against mistakes; the trust boundary is access to the database, which this command
//! signs in to directly (ADR-0040, `v5.0.0-6.4.1`).

use tetsu_identity::{
    Actor, Disabling, EnrollmentToken, OperatorId, OperatorName, OperatorStore, OperatorSummary,
    Operators,
};

use crate::settings::{self, Local};

/// An operator command, with what it acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum Operator {
    Create { name: OperatorName },
    Enroll { id: OperatorId },
    List,
    Disable { id: OperatorId },
    Enable { id: OperatorId },
    EndSessions { id: OperatorId },
    EndAllSessions,
}

const USAGE: &str = "operator takes create <name>, enroll <id>, list, disable <id>, enable <id>, or end-sessions <id \
                     | --all>";

/// The operator command its words name, with its arguments checked.
pub fn parse(rest: &[String]) -> Result<Operator, String> {
    let Some((command, arguments)) = rest.split_first() else {
        return Err(USAGE.into());
    };
    match (command.as_str(), arguments) {
        // A name like an option, such as --help, is a mistake, never an operator to create and enroll.
        ("create", [name]) if name.starts_with('-') => Err(USAGE.into()),
        ("create", [name]) => OperatorName::parse(name)
            .map(|name| Operator::Create { name })
            .map_err(|error| error.to_string()),
        ("list", []) => Ok(Operator::List),
        ("end-sessions", [all]) if all == "--all" => Ok(Operator::EndAllSessions),
        (command, [text]) => with_id(command, text),
        _ => Err(USAGE.into()),
    }
}

/// A command that acts on one operator, with that operator's ID.
fn with_id(command: &str, text: &str) -> Result<Operator, String> {
    let named: fn(OperatorId) -> Operator = match command {
        "enroll" => |id| Operator::Enroll { id },
        "disable" => |id| Operator::Disable { id },
        "enable" => |id| Operator::Enable { id },
        "end-sessions" => |id| Operator::EndSessions { id },
        _ => return Err(USAGE.into()),
    };
    OperatorId::parse(text)
        .map(named)
        .ok_or(format!("{text:?} is not an operator ID"))
}

/// Runs an operator command, and returns the lines it reports. `terminal` says whether those lines go to a
/// terminal, which a command that issues a link requires.
pub async fn run(operator: Operator, local: &Local, terminal: bool) -> Result<Vec<String>, String> {
    let issues_link = matches!(operator, Operator::Create { .. } | Operator::Enroll { .. });
    if issues_link && !terminal {
        return Err(
            "an enrollment link prints only to a terminal, never to a log or a pipe (ADR-0040); run this in one"
                .into(),
        );
    }
    let origin = if issues_link {
        settings::origin(|name| local.var(name))?
    } else {
        String::new()
    };
    let hostname = std::fs::read_to_string("/etc/hostname").ok();
    let by = Actor::from_login(
        login(|name| std::env::var(name).ok()).as_deref(),
        hostname.as_deref().and_then(host_name),
    );
    let operators = tetsu_identity::postgres_store(crate::connect(local).await?);
    act(&operators, operator, &origin, &by).await
}

/// Who is signed in to run the command: `USER`, or `LOGNAME` where `USER` is unset or blank, read from the
/// process's own environment and never from `.env`, which holds settings, not who someone is. It is asserted, not
/// proven: database access is what grants the command (ADR-0061).
fn login(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    ["USER", "LOGNAME"]
        .into_iter()
        .find_map(|name| var(name).filter(|value| !value.trim().is_empty()))
}

/// The machine's name from `/etc/hostname`: its first line that is neither blank nor a `#` comment.
fn host_name(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
}

/// Does what an operator command asks, through Identity's use cases.
async fn act(
    operators: &Operators<impl OperatorStore>,
    operator: Operator,
    origin: &str,
    by: &Actor,
) -> Result<Vec<String>, String> {
    let said = match operator {
        Operator::Create { name } => return create(operators, &name, origin, by).await,
        Operator::Enroll { id } => operators
            .enroll(id, by)
            .await
            .map(|token| vec![link(origin, &token)]),
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
                format!("enabled operator {}, who signs in afresh", id.get())
            } else {
                format!("operator {} was already enabled", id.get())
            }]
        }),
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

/// Creates an operator and issues their first enrollment link. These are two steps, so if the second fails the
/// operator exists without a link, and the error names them so the link can be issued again.
async fn create(
    operators: &Operators<impl OperatorStore>,
    name: &OperatorName,
    origin: &str,
    by: &Actor,
) -> Result<Vec<String>, String> {
    let id = operators
        .create(name, by)
        .await
        .map_err(|error| error.to_string())?;
    let token = operators.enroll(id, by).await.map_err(|error| {
        format!(
            "created operator {id}, but could not issue their enrollment link: {error}; run tetsu operator enroll {id}",
            id = id.get()
        )
    })?;
    Ok(vec![
        format!("created operator {}, {}", id.get(), name.as_str()),
        link(origin, &token),
    ])
}

/// What disabling an operator did, with how many sessions and enrollment links it ended.
fn disabled(id: OperatorId, done: Disabling) -> String {
    if done.done {
        format!(
            "disabled operator {}, ending {} sessions and {} enrollment links",
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
    format!("{}  {}  {state}", operator.id.get(), operator.name)
}

/// The enrollment link, with the token in the fragment, which a browser never sends to the server or in a
/// `Referer`, so it stays out of every log.
fn link(origin: &str, token: &EnrollmentToken) -> String {
    format!(
        "enrollment link, good once and for an hour: {origin}/ops/enroll#{}",
        token.to_text()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args;

    #[test]
    fn operator_commands_are_parsed_exactly() {
        let kurt = OperatorId::parse("7").unwrap();
        for (words, parsed) in [
            (
                &["create", "Kurt Medley"][..],
                Operator::Create {
                    name: OperatorName::parse("Kurt Medley").unwrap(),
                },
            ),
            (&["enroll", "7"], Operator::Enroll { id: kurt }),
            (&["list"], Operator::List),
            (&["disable", "7"], Operator::Disable { id: kurt }),
            (&["enable", "7"], Operator::Enable { id: kurt }),
            (&["end-sessions", "7"], Operator::EndSessions { id: kurt }),
            (&["end-sessions", "--all"], Operator::EndAllSessions),
        ] {
            assert_eq!(parse(&args(words)), Ok(parsed), "{words:?}");
        }
        for bad in [
            &[][..],
            &["create"],
            &["list", "all"],
            &["remove", "7"],
            &["end-sessions"],
            &["create", "--help"],
            &["create", "-x"],
        ] {
            assert_eq!(parse(&args(bad)), Err(USAGE.into()), "{bad:?}");
        }
        assert_eq!(
            parse(&args(&["disable", "seven"])),
            Err("\"seven\" is not an operator ID".into())
        );
        assert!(parse(&args(&["create", ""])).is_err(), "an empty name");
    }

    /// A link is issued only to a terminal, checked before anything is created or read.
    #[tokio::test]
    async fn a_link_is_issued_only_to_a_terminal() {
        let local = Local::default();
        for operator in [
            Operator::Create {
                name: OperatorName::parse("Kurt Medley").unwrap(),
            },
            Operator::Enroll {
                id: OperatorId::parse("7").unwrap(),
            },
        ] {
            let refused = run(operator, &local, false).await.unwrap_err();
            assert!(
                refused.starts_with("an enrollment link prints only to a terminal"),
                "{refused}"
            );
        }
    }

    /// Without the site's origin there is nowhere for a link to point, so nothing is created.
    #[tokio::test]
    async fn a_link_needs_the_sites_origin() {
        let refused = run(
            Operator::Enroll {
                id: OperatorId::parse("7").unwrap(),
            },
            &Local::default(),
            true,
        )
        .await;
        assert_eq!(refused, Err("TETSU_ORIGIN must be set".into()));
    }

    /// Who runs the command is `USER`, or `LOGNAME` when `USER` is unset or blank.
    #[test]
    fn the_login_is_user_or_else_logname() {
        let from = |user: Option<&str>, logname: Option<&str>| {
            login(|name| match name {
                "USER" => user.map(String::from),
                _ => logname.map(String::from),
            })
        };
        assert_eq!(from(Some("kurt"), Some("other")), Some("kurt".into()));
        assert_eq!(from(Some(" "), Some("kurt")), Some("kurt".into()));
        assert_eq!(from(None, Some("kurt")), Some("kurt".into()));
        assert_eq!(from(None, None), None);
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

    fn by() -> Actor {
        Actor::parse("kurt@workstation").unwrap()
    }

    /// What a command said, as the maintainer ran it.
    async fn said(operators: &Operators<impl OperatorStore>, operator: Operator) -> Vec<String> {
        act(operators, operator, ORIGIN, &by()).await.unwrap()
    }

    /// Identity's use cases over a fresh, migrated database, kept while the test runs.
    async fn migrated() -> (
        Operators<impl OperatorStore>,
        tetsu_postgres::test_support::TestDatabase,
    ) {
        let database = tetsu_postgres::test_support::fresh_database()
            .await
            .unwrap();
        tetsu_postgres::migrate(&database, &[&tetsu_identity::Identity])
            .await
            .unwrap();
        let pool = tetsu_postgres::pool(&database.as_app()).unwrap();
        (tetsu_identity::postgres_store(pool), database)
    }

    /// Each command acts through Identity's use cases, against a real database, and says what it did.
    #[tokio::test]
    async fn each_command_acts_and_says_what_it_did() {
        let (operators, _database) = migrated().await;
        let kurt = OperatorId::parse("1").unwrap();
        let created = said(
            &operators,
            Operator::Create {
                name: OperatorName::parse("Kurt Medley").unwrap(),
            },
        )
        .await;
        assert_eq!(created[0], "created operator 1, Kurt Medley");
        let prefix =
            "enrollment link, good once and for an hour: https://ops.example.com/ops/enroll#";
        for line in [
            &created[1],
            &said(&operators, Operator::Enroll { id: kurt }).await[0],
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
            said(&operators, Operator::List).await,
            ["1  Kurt Medley  enabled"]
        );
        assert_eq!(
            said(&operators, Operator::EndSessions { id: kurt }).await,
            ["ended 0 sessions of operator 1"]
        );
        assert_eq!(
            said(&operators, Operator::EndAllSessions).await,
            ["ended 0 operator sessions"]
        );
    }

    /// Disabling and enabling say what they did, and a disabled operator is issued no link.
    #[tokio::test]
    async fn an_operator_is_disabled_and_enabled_again() {
        let (operators, _database) = migrated().await;
        let kurt = OperatorId::parse("1").unwrap();
        said(
            &operators,
            Operator::Create {
                name: OperatorName::parse("Kurt Medley").unwrap(),
            },
        )
        .await;
        assert_eq!(
            said(&operators, Operator::Disable { id: kurt }).await,
            ["disabled operator 1, ending 0 sessions and 1 enrollment links"]
        );
        assert_eq!(
            said(&operators, Operator::Disable { id: kurt }).await,
            ["operator 1 was already disabled"]
        );
        assert_eq!(
            said(&operators, Operator::List).await,
            ["1  Kurt Medley  disabled"]
        );
        assert_eq!(
            act(&operators, Operator::Enroll { id: kurt }, ORIGIN, &by()).await,
            Err("invalid: no enabled operator 1".into())
        );
        assert_eq!(
            said(&operators, Operator::Enable { id: kurt }).await,
            ["enabled operator 1, who signs in afresh"]
        );
        assert_eq!(
            said(&operators, Operator::Enable { id: kurt }).await,
            ["operator 1 was already enabled"]
        );
        let nobody = OperatorId::parse("9").unwrap();
        for command in [
            Operator::Disable { id: nobody },
            Operator::Enable { id: nobody },
            Operator::EndSessions { id: nobody },
        ] {
            assert_eq!(
                act(&operators, command, ORIGIN, &by()).await,
                Err("invalid: no operator 9".into())
            );
        }
    }

    /// When the link cannot be issued after the operator is made, the error names the operator, so a link can be
    /// issued again rather than a second operator made.
    #[tokio::test]
    async fn a_failed_link_after_creating_names_the_new_operator() {
        let (operators, database) = migrated().await;
        tetsu_postgres::connect(&database)
            .await
            .unwrap()
            .batch_execute(
                "REVOKE EXECUTE ON FUNCTION identity.issue_enrollment(bytea, bigint) FROM tetsu_identity",
            )
            .await
            .unwrap();
        let by = Actor::parse("kurt@workstation").unwrap();
        let created = Operator::Create {
            name: OperatorName::parse("Kurt Medley").unwrap(),
        };
        let refused = act(&operators, created, "https://ops.example.com", &by)
            .await
            .unwrap_err();
        assert!(
            refused.starts_with("created operator 1, but could not issue their enrollment link: ")
                && refused.ends_with("; run tetsu operator enroll 1"),
            "{refused}"
        );
    }
}
