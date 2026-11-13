//! Domain events through a transactional outbox (ADR-0043). A module records an event in its own outbox in the
//! transaction that makes the change, and the dispatcher hands it to every consumer that wants it, at least once,
//! while each consumer's inbox makes its effect happen once.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use deadpool_postgres::{Pool, Transaction};
use tokio::sync::Notify;

use crate::connection::Error;
use crate::scope::{DeliveryTransaction, ModuleTransaction, TenantTransaction};

/// A module's outboxes, as one of its migrations.
pub const OUTBOX: &str = include_str!("../migrations/outbox.sql");
/// A consuming module's inboxes, as one of its migrations.
pub const INBOX: &str = include_str!("../migrations/inbox.sql");
/// After this many failed attempts an event stops, unless it is kept, and its failure is logged as an error.
pub const MAX_ATTEMPTS: i64 = 10;
/// How long a delivered event stays before it is deleted. The outbox delivers; it is no history.
pub const KEPT_FOR: Duration = Duration::from_hours(7 * 24);
/// How often the dispatcher removes delivered events past [`KEPT_FOR`].
const CLEAN_EVERY: Duration = Duration::from_hours(1);
/// How many events one pass takes from one outbox.
const BATCH: i64 = 50;

/// Wakes the dispatcher when this process commits an event, so it need not wait for its next poll.
static RECORDED: Notify = Notify::const_new();

/// An event to record: a fact in the past tense, such as `identity.signed_in`, its version, and a JSON payload of
/// IDs and facts, never personal details.
pub struct NewEvent<'a> {
    pub kind: &'a str,
    pub version: i64,
    pub payload: &'a str,
    /// Never stop retrying it, as for a recovery notice (ADR-0041).
    pub keep: bool,
}

/// An event as a consumer receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    /// The schema of the module that recorded it.
    pub producer: String,
    /// The business it belongs to, or none for a global event.
    pub tenant: Option<i64>,
    pub kind: String,
    pub version: i64,
    pub occurred_at: SystemTime,
    pub payload: String,
}

pub type Applied<'a> = Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

/// A module that reacts to events, through a use case of its own. It applies each event inside a transaction that
/// acts as its own role, for the event's tenant when it has one, and never reads another module's tables.
pub trait Consumer: Send + Sync {
    /// The consuming module's schema, which holds its inboxes.
    fn schema(&self) -> &'static str;
    /// Whether it wants this event at all.
    fn wants(&self, event: &Event) -> bool;
    /// Applies the event. It may see the same event again, and events in any order.
    fn apply<'a>(&'a self, transaction: &'a Transaction<'a>, event: &'a Event) -> Applied<'a>;
}

impl TenantTransaction<'_> {
    /// Records an event in this module's outbox for this transaction's tenant. It is delivered only if the
    /// transaction commits.
    ///
    /// # Errors
    /// Fails if the payload is not JSON or the insert fails.
    pub async fn record(&self, event: &NewEvent<'_>) -> Result<String, tokio_postgres::Error> {
        self.recorded();
        insert(self, self.schema(), Outbox::Tenant, event).await
    }
}

impl ModuleTransaction<'_> {
    /// Records an event that belongs to no business in this module's global outbox.
    ///
    /// # Errors
    /// Fails if the payload is not JSON or the insert fails.
    pub async fn record(&self, event: &NewEvent<'_>) -> Result<String, tokio_postgres::Error> {
        self.recorded();
        insert(self, self.schema(), Outbox::Global, event).await
    }
}

/// Inserts an event into one of `schema`'s outboxes, the tenant one taking the transaction's tenant.
async fn insert(
    transaction: &Transaction<'_>,
    schema: &str,
    outbox: Outbox,
    event: &NewEvent<'_>,
) -> Result<String, tokio_postgres::Error> {
    let (column, value) = match outbox {
        Outbox::Tenant => (
            "tenant_id, ",
            "(SELECT nullif(current_setting('app.tenant_id', true), '')::bigint), ",
        ),
        Outbox::Global => ("", ""),
    };
    let sql = format!(
        r#"INSERT INTO "{schema}".{} ({column}event_type, event_version, payload, keep)
           VALUES ({value}$1, $2, $3::text::jsonb, $4) RETURNING id::text"#,
        outbox.table()
    );
    transaction
        .query_one(
            &sql,
            &[&event.kind, &event.version, &event.payload, &event.keep],
        )
        .await?
        .try_get(0)
}

/// Lets the dispatcher know at once that an event committed.
pub(crate) fn wake() {
    RECORDED.notify_one();
}

/// Delivers every producer's events to the consumers that want them.
pub struct Dispatcher {
    pool: Pool,
    producers: Vec<String>,
    consumers: Vec<Arc<dyn Consumer>>,
}

/// One outbox: a module's tenant-owned or global table.
#[derive(Clone, Copy)]
enum Outbox {
    Tenant,
    Global,
}

impl Outbox {
    fn table(self) -> &'static str {
        match self {
            Self::Tenant => "outbox",
            Self::Global => "outbox_global",
        }
    }
}

impl Dispatcher {
    /// A dispatcher for the modules in `schemas` that have outboxes, found once, here.
    ///
    /// # Errors
    /// Fails if the database cannot be reached.
    pub async fn new(
        pool: Pool,
        schemas: &[&str],
        consumers: Vec<Arc<dyn Consumer>>,
    ) -> Result<Self, Error> {
        let client = pool.get().await?;
        let producers = client
            .query(
                "SELECT n.nspname::text FROM pg_catalog.pg_class c
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                 WHERE c.relname = 'outbox' AND c.relkind = 'r' AND n.nspname = any($1)
                 ORDER BY 1",
                &[&schemas],
            )
            .await?
            .iter()
            .map(|row| row.try_get(0))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            pool,
            producers,
            consumers,
        })
    }

    /// Delivers until `stop` finishes: every `poll`, and at once when this process records an event. Removes
    /// delivered events past [`KEPT_FOR`] every [`CLEAN_EVERY`]. A failed pass is logged and tried again on the next.
    pub async fn run(&self, poll: Duration, stop: impl Future<Output = ()>) {
        tokio::pin!(stop);
        let mut cleaned: Option<Instant> = None;
        loop {
            if let Err(error) = self.deliver_due().await {
                tracing::error!(error = %error, "event delivery failed");
            }
            if cleaned.is_none_or(|at| at.elapsed() >= CLEAN_EVERY) {
                match self.remove_delivered(KEPT_FOR).await {
                    Ok(_) => cleaned = Some(Instant::now()),
                    Err(error) => {
                        tracing::error!(error = %error, "removing delivered events failed");
                    }
                }
            }
            tokio::select! {
                () = &mut stop => return,
                () = tokio::time::sleep(poll) => {}
                () = RECORDED.notified() => {}
            }
        }
    }

    /// One pass over every outbox: each due event goes to every consumer that wants it, and is settled.
    ///
    /// # Errors
    /// Fails if an outbox cannot be read or settled. A consumer's failure is not an error here; it schedules a retry.
    pub async fn deliver_due(&self) -> Result<usize, Error> {
        let mut delivered = 0;
        for producer in &self.producers {
            for outbox in [Outbox::Tenant, Outbox::Global] {
                delivered += self.deliver_from(producer, outbox).await?;
            }
        }
        Ok(delivered)
    }

    async fn deliver_from(&self, producer: &str, outbox: Outbox) -> Result<usize, Error> {
        let table = format!(r#""{producer}".{}"#, outbox.table());
        let mut client = self.pool.get().await?;
        let transaction = DeliveryTransaction::begin(&mut client).await?;
        let rows = due(&transaction, &table, outbox).await?;
        let mut delivered = 0;
        for row in &rows {
            let (event, keep) = read_event(row, producer)?;
            let failed = self.hand_over(&event).await;
            if failed.is_empty() {
                settle(&transaction, &table, &event).await?;
                delivered += 1;
            } else {
                retry_later(&transaction, &table, &event, keep, &failed).await?;
            }
        }
        transaction.commit().await?;
        Ok(delivered)
    }

    /// Gives the event to each consumer that wants it, and returns the schemas of those that failed.
    async fn hand_over(&self, event: &Event) -> Vec<&'static str> {
        let mut failed = Vec::new();
        for consumer in self
            .consumers
            .iter()
            .filter(|consumer| consumer.wants(event))
        {
            if let Err(error) = self.apply_once(consumer.as_ref(), event).await {
                tracing::warn!(
                    event_id = event.id,
                    event_type = event.kind,
                    consumer = consumer.schema(),
                    error = %error,
                    "event not applied"
                );
                failed.push(consumer.schema());
            }
        }
        failed
    }

    /// Applies an event in the consumer's own transaction, which also records it in the consumer's inbox, so an
    /// event already applied is skipped.
    async fn apply_once(&self, consumer: &dyn Consumer, event: &Event) -> Result<(), Error> {
        let schema = consumer.schema();
        let mut client = self.pool.get().await?;
        let transaction = ModuleTransaction::begin(&mut client, schema).await?;
        if let Some(tenant) = event.tenant {
            let transaction = transaction.for_tenant(tenant).await?;
            let inbox = format!(
                r#"INSERT INTO "{schema}".inbox (tenant_id, id)
                   VALUES ((SELECT nullif(current_setting('app.tenant_id', true), '')::bigint), $1::text::uuid)
                   ON CONFLICT DO NOTHING"#
            );
            once(&transaction, &inbox, consumer, event).await?;
            transaction.commit().await?;
        } else {
            let inbox = format!(
                r#"INSERT INTO "{schema}".inbox_global (id) VALUES ($1::text::uuid) ON CONFLICT DO NOTHING"#
            );
            once(&transaction, &inbox, consumer, event).await?;
            transaction.commit().await?;
        }
        Ok(())
    }

    /// Deletes events delivered more than `age` ago, and returns how many.
    ///
    /// # Errors
    /// Fails if an outbox cannot be cleaned.
    pub async fn remove_delivered(&self, age: Duration) -> Result<u64, Error> {
        let seconds = age.as_secs_f64();
        let mut removed = 0;
        let mut client = self.pool.get().await?;
        let transaction = DeliveryTransaction::begin(&mut client).await?;
        for producer in &self.producers {
            for outbox in [Outbox::Tenant, Outbox::Global] {
                removed += transaction
                    .execute(
                        &format!(
                            r#"DELETE FROM "{producer}".{} WHERE delivered_at < now() - make_interval(secs => $1)"#,
                            outbox.table()
                        ),
                        &[&seconds],
                    )
                    .await?;
            }
        }
        transaction.commit().await?;
        Ok(removed)
    }
}

/// The outbox's due events, oldest first, locked for this transaction. Rows another instance holds are skipped,
/// never waited on.
async fn due(
    transaction: &Transaction<'_>,
    table: &str,
    outbox: Outbox,
) -> Result<Vec<tokio_postgres::Row>, tokio_postgres::Error> {
    let tenant = match outbox {
        Outbox::Tenant => "tenant_id",
        Outbox::Global => "NULL::bigint",
    };
    transaction
        .query(
            &format!(
                "SELECT id::text, {tenant}, event_type, event_version, occurred_at, payload::text, keep FROM {table}
                 WHERE delivered_at IS NULL AND due_at <= now() AND (keep OR attempts < $1)
                 ORDER BY due_at LIMIT $2 FOR UPDATE SKIP LOCKED"
            ),
            &[&MAX_ATTEMPTS, &BATCH],
        )
        .await
}

/// An outbox row as an event, and whether it is kept.
fn read_event(row: &tokio_postgres::Row, producer: &str) -> Result<(Event, bool), Error> {
    let event = Event {
        id: row.try_get(0)?,
        producer: producer.to_string(),
        tenant: row.try_get(1)?,
        kind: row.try_get(2)?,
        version: row.try_get(3)?,
        occurred_at: row.try_get(4)?,
        payload: row.try_get(5)?,
    };
    Ok((event, row.try_get(6)?))
}

/// Marks an event delivered to every consumer that wants it.
async fn settle(transaction: &Transaction<'_>, table: &str, event: &Event) -> Result<(), Error> {
    transaction
        .execute(
            &format!("UPDATE {table} SET delivered_at = now() WHERE id = $1::text::uuid"),
            &[&event.id],
        )
        .await?;
    Ok(())
}

/// Records the event in the consumer's inbox with `inbox`, and applies it unless the inbox already held it.
async fn once(
    transaction: &Transaction<'_>,
    inbox: &str,
    consumer: &dyn Consumer,
    event: &Event,
) -> Result<(), Error> {
    if transaction.execute(inbox, &[&event.id]).await? == 1 {
        consumer.apply(transaction, event).await?;
    }
    Ok(())
}

/// Schedules the next attempt, with exponential backoff and jitter capped at an hour, and logs an error once an
/// event that is not kept runs out of attempts.
async fn retry_later(
    transaction: &Transaction<'_>,
    table: &str,
    event: &Event,
    keep: bool,
    failed: &[&str],
) -> Result<(), Error> {
    let attempts: i64 = transaction
        .query_one(
            &format!(
                "UPDATE {table} SET attempts = attempts + 1,
                   due_at = now() + least(interval '1 hour', interval '1 second' * power(2, attempts))
                                   * (0.5 + random() / 2)
                 WHERE id = $1::text::uuid RETURNING attempts"
            ),
            &[&event.id],
        )
        .await?
        .try_get(0)?;
    if attempts >= MAX_ATTEMPTS && !keep {
        for consumer in failed {
            tracing::error!(
                event_id = event.id,
                event_type = event.kind,
                consumer,
                attempts,
                "event stopped retrying"
            );
        }
    }
    Ok(())
}
