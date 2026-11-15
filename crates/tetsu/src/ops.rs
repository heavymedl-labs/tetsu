//! The hosted chores, run by workflows rather than by hand against the hosted database (#135): checking isolation
//! after a migrate, starting the demo over, replaying a stopped event, and moving a tenant off a reserved address.
//! Each says what it changed, as a line the workflow's log keeps.

use tetsu_kernel::Module;

use crate::settings::{self, Local};

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
