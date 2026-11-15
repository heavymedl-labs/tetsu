//! The operator command (ADR-0040, ADR-0059): creates operators and issues their enrollment links, lists them,
//! disables them, and ends their sessions. Each change is recorded for the audit record by Identity.
//!
//! An enrollment link is printed only to a terminal, never to a log, a pipe, or CI's output, so whoever ran the
//! command is the only one who sees it (ADR-0040, `v5.0.0-6.4.1`).

use tetsu_identity::{
    EnrollmentToken, OperatorId, OperatorName, OperatorStore, OperatorSummary, Operators,
};

use crate::settings::{self, Local};

/// An operator command, with what it acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum Operator {
    Create { name: OperatorName },
    Enroll { id: OperatorId },
    List,
    Disable { id: OperatorId },
    EndSessions { id: OperatorId },
    EndAllSessions,
}

const USAGE: &str =
    "operator takes create <name>, enroll <id>, list, disable <id>, or end-sessions <id | --all>";

/// The operator command its words name, with its arguments checked.
pub fn parse(rest: &[String]) -> Result<Operator, String> {
    let Some((command, arguments)) = rest.split_first() else {
        return Err(USAGE.into());
    };
    match (command.as_str(), arguments) {
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
    let operators = tetsu_identity::postgres_store(crate::connect(local).await?);
    act(&operators, operator, &origin)
        .await
        .map_err(|error| error.to_string())
}

/// Does what an operator command asks, through Identity's use cases.
async fn act(
    operators: &Operators<impl OperatorStore>,
    operator: Operator,
    origin: &str,
) -> Result<Vec<String>, tetsu_identity::Error> {
    match operator {
        Operator::Create { name } => create(operators, &name, origin).await,
        Operator::Enroll { id } => operators
            .enroll(id)
            .await
            .map(|token| vec![link(origin, &token)]),
        Operator::List => operators
            .list()
            .await
            .map(|all| all.iter().map(listed).collect()),
        Operator::Disable { id } => operators
            .disable(id)
            .await
            .map(|done| vec![disabled(id, done)]),
        Operator::EndSessions { id } => operators
            .end_sessions(id)
            .await
            .map(|ended| vec![format!("ended {ended} sessions of operator {}", id.get())]),
        Operator::EndAllSessions => operators
            .end_all_sessions()
            .await
            .map(|ended| vec![format!("ended {ended} operator sessions")]),
    }
}

/// Creates an operator and issues their first enrollment link.
async fn create(
    operators: &Operators<impl OperatorStore>,
    name: &OperatorName,
    origin: &str,
) -> Result<Vec<String>, tetsu_identity::Error> {
    let id = operators.create(name).await?;
    let token = operators.enroll(id).await?;
    Ok(vec![
        format!("created operator {}, {}", id.get(), name.as_str()),
        link(origin, &token),
    ])
}

/// What disabling an operator did.
fn disabled(id: OperatorId, done: bool) -> String {
    if done {
        format!(
            "disabled operator {}, and ended their sessions and enrollment link",
            id.get()
        )
    } else {
        format!(
            "operator {} was already disabled, or does not exist",
            id.get()
        )
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
        let origin = "https://ops.example.com";
        let kurt = OperatorId::parse("1").unwrap();
        let said = async |operator| act(&operators, operator, origin).await.unwrap();
        let created = said(Operator::Create {
            name: OperatorName::parse("Kurt Medley").unwrap(),
        })
        .await;
        assert_eq!(created[0], "created operator 1, Kurt Medley");
        let prefix =
            "enrollment link, good once and for an hour: https://ops.example.com/ops/enroll#";
        for line in [&created[1], &said(Operator::Enroll { id: kurt }).await[0]] {
            let token = line
                .strip_prefix(prefix)
                .unwrap_or_else(|| panic!("{line}"));
            assert!(
                token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()),
                "{token}"
            );
        }
        assert_eq!(said(Operator::List).await, ["1  Kurt Medley  enabled"]);
        assert_eq!(
            said(Operator::EndSessions { id: kurt }).await,
            ["ended 0 sessions of operator 1"]
        );
        assert_eq!(
            said(Operator::EndAllSessions).await,
            ["ended 0 operator sessions"]
        );
        assert_eq!(
            said(Operator::Disable { id: kurt }).await,
            ["disabled operator 1, and ended their sessions and enrollment link"]
        );
        assert_eq!(
            said(Operator::Disable { id: kurt }).await,
            ["operator 1 was already disabled, or does not exist"]
        );
        assert_eq!(said(Operator::List).await, ["1  Kurt Medley  disabled"]);
        assert_eq!(
            act(&operators, Operator::Enroll { id: kurt }, origin).await,
            Err(tetsu_identity::Error::Invalid(
                "no enabled operator 1".into()
            ))
        );
    }
}
