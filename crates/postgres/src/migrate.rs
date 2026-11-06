//! The migration runner.

use std::time::{Duration, Instant};

use tetsu_kernel::{Migration, Module};
use tokio_postgres::Client;

use crate::connection::{Error, Settings, connect};

/// Taken by polling, never by blocking, which can deadlock with
/// `CREATE INDEX CONCURRENTLY` (ADR-0011).
pub const MIGRATION_LOCK: i64 = 7_243_001;
const LOCK_WAIT: Duration = Duration::from_secs(60);
const LOCK_POLL: Duration = Duration::from_millis(100);
const NO_TRANSACTION: &str = "-- tetsu:no-transaction";

/// Applies each module's new migrations.
///
/// # Errors
/// Fails if the lock stays held, a file fails, or an applied file has changed.
pub async fn migrate(settings: &Settings, modules: &[&dyn Module]) -> Result<(), Error> {
    migrate_with_wait(settings, modules, LOCK_WAIT).await
}

/// [`migrate`], with a chosen wait for the lock.
///
/// # Errors
/// As [`migrate`].
pub async fn migrate_with_wait(
    settings: &Settings,
    modules: &[&dyn Module],
    wait: Duration,
) -> Result<(), Error> {
    let client = connect(settings).await?;
    take_lock(&client, wait).await?;
    for module in modules {
        if let Some(storage) = module.storage() {
            migrate_module(&client, storage.schema, storage.migrations).await?;
        }
    }
    // Closing the connection releases the lock, even after a failure above.
    Ok(())
}

async fn take_lock(client: &Client, wait: Duration) -> Result<(), Error> {
    let started = Instant::now();
    loop {
        let row = client
            .query_one(
                "SELECT pg_try_advisory_lock($1) AS taken",
                &[&MIGRATION_LOCK],
            )
            .await?;
        if row.try_get::<_, bool>("taken")? {
            return Ok(());
        }
        if started.elapsed() >= wait {
            return Err("another migration run still holds the lock".into());
        }
        tokio::time::sleep(LOCK_POLL).await;
    }
}

async fn migrate_module(
    client: &Client,
    schema: &str,
    migrations: &[Migration],
) -> Result<(), Error> {
    client
        .batch_execute(&format!(
            "CREATE SCHEMA IF NOT EXISTS {schema};
             CREATE TABLE IF NOT EXISTS {schema}.migrations (
                 version int PRIMARY KEY,
                 sql text NOT NULL,
                 applied_at timestamptz NOT NULL DEFAULT now()
             )"
        ))
        .await?;
    for migration in migrations {
        let applied = client
            .query_opt(
                &format!("SELECT sql FROM {schema}.migrations WHERE version = $1"),
                &[&migration.version],
            )
            .await?;
        match applied {
            Some(row) if row.try_get::<_, String>("sql")? == migration.sql => {}
            Some(_) => {
                return Err(format!(
                    "{schema} version {} changed after it was applied",
                    migration.version
                )
                .into());
            }
            None => apply(client, schema, migration).await?,
        }
    }
    Ok(())
}

async fn apply(client: &Client, schema: &str, migration: &Migration) -> Result<(), Error> {
    let record = format!(
        "INSERT INTO {schema}.migrations (version, sql) VALUES ({}, $1)",
        migration.version
    );
    if let Some(statement) = migration.sql.strip_prefix(NO_TRANSACTION) {
        return apply_outside_transaction(client, schema, migration, statement, &record).await;
    }
    // The change and its record commit together, so a crash cannot leave one without the other.
    client.batch_execute("BEGIN").await?;
    let applied = async {
        client.batch_execute(migration.sql).await?;
        client.execute(&record, &[&migration.sql]).await
    };
    if let Err(error) = applied.await {
        client.batch_execute("ROLLBACK").await?;
        return Err(error.into());
    }
    client.batch_execute("COMMIT").await?;
    Ok(())
}

/// For statements Postgres refuses to run in a transaction, such as `CREATE INDEX CONCURRENTLY`.
async fn apply_outside_transaction(
    client: &Client,
    schema: &str,
    migration: &Migration,
    statement: &str,
    record: &str,
) -> Result<(), Error> {
    if statement.trim().trim_end_matches(';').contains(';') {
        return Err(format!(
            "{schema} version {}: a file outside a transaction holds one statement",
            migration.version
        )
        .into());
    }
    client.batch_execute(migration.sql).await?;
    refuse_invalid_indexes(client, schema).await?;
    client.execute(record, &[&migration.sql]).await?;
    Ok(())
}

/// A failed `CREATE INDEX CONCURRENTLY` leaves an invalid index behind.
async fn refuse_invalid_indexes(client: &Client, schema: &str) -> Result<(), Error> {
    let invalid = client
        .query(
            "SELECT i.indexrelid::regclass::text AS name FROM pg_index i
             JOIN pg_class c ON c.oid = i.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = $1 AND NOT i.indisvalid",
            &[&schema],
        )
        .await?;
    match invalid.first() {
        Some(row) => Err(format!(
            "invalid index {} needs dropping before a rerun",
            row.try_get::<_, String>("name")?
        )
        .into()),
        None => Ok(()),
    }
}
