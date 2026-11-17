//! Creating the database a developer's settings name, for `tetsu setup` (ADR-0050): on a local server the database
//! may be gone, as when a test server keeps its data in memory and restarts.

use crate::connection::{Error, Settings, connect};
use crate::roles::quoted;

/// Creates the database `settings` name if the server has none by that name, through the same server's `postgres`
/// database, and says whether it did. It never drops or changes one that exists, so it is safe to run any time.
///
/// # Errors
/// Fails if the settings are not a URL naming a database, the server cannot be reached, or the role may not create
/// a database.
pub async fn create_database(settings: &Settings) -> Result<bool, Error> {
    let config: tokio_postgres::Config = settings.url().parse()?;
    let name = config
        .get_dbname()
        .ok_or("the database settings name no database")?;
    let server = connect(&settings.on_database("postgres")?).await?;
    let found: bool = server
        .query_one(
            "SELECT EXISTS (SELECT FROM pg_database WHERE datname = $1)",
            &[&name],
        )
        .await?
        .try_get(0)?;
    if !found {
        server
            .batch_execute(&format!("CREATE DATABASE {}", quoted(name)))
            .await?;
    }
    Ok(!found)
}
