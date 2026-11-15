//! The hosted chores, run by workflows rather than by hand against the hosted database (#135): checking isolation
//! after a migrate, starting the demo over, replaying a stopped event, and moving a tenant off a reserved address.
//! Each says what it changed, as a line the workflow's log keeps.

use tetsu_kernel::Module;

use crate::settings::{self, Local};

/// A hosted chore, with what it acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum Chore {
    Check,
    Reset { schemas: Vec<String> },
    Replay { producer: String, id: String },
    Rename { address: String, slug: String },
}

/// The chore a command names, with its arguments checked, or none when the command names no chore.
pub fn parse(command: &str, rest: &[String]) -> Option<Result<Chore, String>> {
    let wrong = |why: &str| Some(Err(why.to_string()));
    match (command, rest) {
        ("check", []) => Some(Ok(Chore::Check)),
        ("check", _) => wrong("check takes no arguments"),
        ("reset", []) => wrong("reset takes one or more module schemas"),
        ("reset", schemas) => Some(Ok(Chore::Reset {
            schemas: schemas.to_vec(),
        })),
        ("replay", [producer, id]) => Some(Ok(Chore::Replay {
            producer: producer.clone(),
            id: id.clone(),
        })),
        ("replay", _) => wrong("replay takes the recording module's schema and the event's ID"),
        ("rename", [address, slug]) => Some(Ok(Chore::Rename {
            address: address.clone(),
            slug: slug.clone(),
        })),
        ("rename", _) => wrong("rename takes the tenant's address and its new slug"),
        _ => None,
    }
}

/// Runs a chore, and returns the lines it reports.
pub async fn run(
    chore: Chore,
    local: &Local,
    modules: &[&dyn Module],
) -> Result<Vec<String>, String> {
    match chore {
        Chore::Check => check(local).await,
        Chore::Reset { schemas } => reset(local, &schemas).await,
        Chore::Replay { producer, id } => replay(local, modules, &producer, &id).await,
        Chore::Rename { address, slug } => rename(local, &address, &slug).await,
    }
}

/// Hosted test 8 after a migrate: each problem on its own line, and an error if there is any.
pub async fn check(local: &Local) -> Result<Vec<String>, String> {
    let settings = settings::database(|name| local.var(name))?;
    let problems = tetsu_postgres::check_isolation(&settings)
        .await
        .map_err(|error| crate::explain(&*error))?;
    if problems.is_empty() {
        return Ok(vec!["no isolation problems".into()]);
    }
    Err(format!(
        "{} isolation problems:\n{}",
        problems.len(),
        problems.join("\n")
    ))
}

/// Drops the named modules' schemas so the next migrate builds them afresh, only where `TETSU_DEMO` is true, since
/// only the demo holds no real business's data.
pub async fn reset(local: &Local, schemas: &[String]) -> Result<Vec<String>, String> {
    if !settings::demo(|name| local.var(name))? {
        return Err("reset runs only where TETSU_DEMO is true, since it drops data".into());
    }
    let settings = settings::database(|name| local.var(name))?;
    tetsu_postgres::drop_module_schemas(&settings, schemas)
        .await
        .map_err(|error| crate::explain(&*error))?;
    Ok(vec![format!("dropped: {}", schemas.join(", "))])
}

/// Makes a stopped event due again, as the delivery role, from the outboxes of the modules the binary runs.
pub async fn replay(
    local: &Local,
    modules: &[&dyn Module],
    producer: &str,
    id: &str,
) -> Result<Vec<String>, String> {
    let pool = crate::connect(local).await?;
    let schemas: Vec<&str> = modules
        .iter()
        .filter_map(|module| module.storage())
        .map(|storage| storage.schema)
        .collect();
    let dispatcher = tetsu_postgres::Dispatcher::new(pool, &schemas, Vec::new())
        .await
        .map_err(|error| crate::explain(&*error))?;
    match dispatcher.replay(producer, id).await {
        Ok(Some(outbox)) => Ok(vec![format!(
            "replayed {id} from {producer}.{outbox}; it is due now"
        )]),
        Ok(None) => Err(format!(
            "no event {id} in {producer}'s outboxes; it may have been delivered and removed"
        )),
        Err(error) => Err(crate::explain(&*error)),
    }
}

/// Moves the tenant at `address` to `slug` and its address, through Tenancy's use case.
pub async fn rename(local: &Local, address: &str, slug: &str) -> Result<Vec<String>, String> {
    let to = tetsu_tenancy::Slug::parse(slug).map_err(|error| error.to_string())?;
    let pool = crate::connect(local).await?;
    tetsu_tenancy::postgres_store(pool)
        .rename(address, &to)
        .await
        .map_err(|error| format!("could not rename the tenant at {address}: {error}"))?;
    Ok(vec![format!(
        "moved the tenant at {address} to {}",
        to.address()
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args;

    /// The hosted chores take exactly what they act on (#135), and any other command is not a chore.
    #[test]
    fn chores_are_parsed_exactly() {
        assert_eq!(parse("check", &[]), Some(Ok(Chore::Check)));
        assert_eq!(
            parse("reset", &args(&["tenancy", "audit"])),
            Some(Ok(Chore::Reset {
                schemas: args(&["tenancy", "audit"])
            }))
        );
        assert_eq!(
            parse("replay", &args(&["identity", "0190"])),
            Some(Ok(Chore::Replay {
                producer: "identity".into(),
                id: "0190".into()
            }))
        );
        assert_eq!(
            parse("rename", &args(&["/login", "sample-gym"])),
            Some(Ok(Chore::Rename {
                address: "/login".into(),
                slug: "sample-gym".into()
            }))
        );
        for (command, rest, why) in [
            ("check", &["now"][..], "check takes no arguments"),
            ("reset", &[], "reset takes one or more module schemas"),
            (
                "replay",
                &["identity"],
                "replay takes the recording module's schema and the event's ID",
            ),
            (
                "rename",
                &["/login"],
                "rename takes the tenant's address and its new slug",
            ),
            (
                "rename",
                &["/login", "a", "b"],
                "rename takes the tenant's address and its new slug",
            ),
        ] {
            assert_eq!(
                parse(command, &args(rest)),
                Some(Err(why.into())),
                "{command} {rest:?}"
            );
        }
        assert_eq!(parse("migrate", &[]), None);
    }
}
