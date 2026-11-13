//! Domain events through a transactional outbox (ADR-0043). A module records an event in its own outbox in the
//! transaction that makes the change, and the dispatcher hands it to every consumer that wants it, at least once,
//! while each consumer's inbox makes its effect happen once.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use std::sync::OnceLock;

use deadpool_postgres::{Object, Pool, Transaction};
use tokio::sync::watch;

use crate::connection::Error;
use crate::scope::{DeliveryTransaction, ModuleTransaction, TenantTransaction};

/// A module's outboxes, as one of its migrations.
pub const OUTBOX: &str = include_str!("../migrations/outbox.sql");
/// A consuming module's inboxes, as one of its migrations.
pub const INBOX: &str = include_str!("../migrations/inbox.sql");
/// After this many failed attempts an event stops, unless it is kept, and its failure is logged as an error either
/// way, so operations can alert on it (#27).
pub const MAX_ATTEMPTS: i64 = 10;
/// How long a consumer may take to apply one event before it counts as failed, so one stuck on an outside call
/// cannot stop delivery.
const APPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a delivered event stays before it is deleted. The outbox delivers; it is no history.
pub const KEPT_FOR: Duration = Duration::from_hours(7 * 24);
/// How often the dispatcher removes delivered events past [`KEPT_FOR`].
const CLEAN_EVERY: Duration = Duration::from_hours(1);
/// How long one pass may hand events over from one outbox before it commits what it has, so a stuck consumer
/// cannot hold the outbox's rows and its connection for long.
const PASS_LIMIT: Duration = Duration::from_mins(1);
/// How many events one pass takes from one outbox.
const BATCH: i64 = 50;

/// Counts the events this process commits, so every dispatcher wakes at once rather than at its next poll. A
/// dispatcher busy when one commits sees the count changed when it next waits, so no wake is lost.
fn recorded() -> &'static watch::Sender<u64> {
    static RECORDED: OnceLock<watch::Sender<u64>> = OnceLock::new();
    RECORDED.get_or_init(|| watch::channel(0).0)
}

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
    /// Whether an event it fails to apply must keep retrying, however often it fails, as the audit record's must
    /// (ADR-0045). An event is marked kept when such a consumer is still failing at its last attempt, and stays
    /// kept after.
    fn never_lose(&self) -> bool {
        false
    }
    /// Applies the event. It may see the same event again, and events in any order. Return a database error as it
    /// came, with `?`, so the log can name it by its SQL state rather than by text that may quote a row's values.
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
    recorded().send_modify(|count| *count = count.wrapping_add(1));
}

/// Delivers every producer's events to the consumers that want them.
pub struct Dispatcher {
    pool: Pool,
    producers: Vec<String>,
    consumers: Vec<Arc<dyn Consumer>>,
    apply_timeout: Duration,
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

    /// Finds one event by `$1`, its ID, and `$2`, its tenant, so the tenant outbox's key is used whole.
    fn row(self) -> &'static str {
        match self {
            Self::Tenant => "tenant_id = $2 AND id = $1::text::uuid",
            Self::Global => "id = $1::text::uuid AND $2::bigint IS NULL",
        }
    }
}

/// What became of one event in a pass.
#[derive(Clone, Copy)]
enum Handed {
    Delivered,
    Retried,
    /// The pool had no connection for a consumer, so the event, and the rest of the pass, wait.
    Starved,
}

/// What one outbox's pass did.
#[derive(Default)]
struct Pass {
    delivered: usize,
    starved: bool,
}

impl Pass {
    fn count(&mut self, handed: Handed) {
        match handed {
            Handed::Delivered => self.delivered += 1,
            Handed::Retried => {}
            Handed::Starved => self.starved = true,
        }
    }
}

/// An error as logs may show it: a database error by its SQL state alone, since its text and detail can quote a
/// row's values, and any other error as it reads.
fn described(error: &(dyn std::error::Error + 'static)) -> String {
    std::iter::successors(Some(error), |error| error.source())
        .find_map(|error| error.downcast_ref::<tokio_postgres::Error>())
        .and_then(tokio_postgres::Error::code)
        .map_or_else(
            || error.to_string(),
            |code| format!("database error {}", code.code()),
        )
}

/// One producer's outbox, as one pass reads and settles it.
struct Place<'a> {
    producer: &'a str,
    table: String,
    outbox: Outbox,
}

impl<'a> Place<'a> {
    fn new(producer: &'a str, outbox: Outbox) -> Self {
        Self {
            producer,
            table: format!(r#""{producer}".{}"#, outbox.table()),
            outbox,
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
            apply_timeout: APPLY_TIMEOUT,
        })
    }

    /// The same dispatcher, giving each consumer `timeout` to apply an event, so tests need not wait out the real
    /// one.
    #[doc(hidden)]
    #[must_use]
    pub fn with_apply_timeout(self, timeout: Duration) -> Self {
        Self {
            apply_timeout: timeout,
            ..self
        }
    }

    /// Delivers until `stop` finishes: every `poll`, and at once when this process records an event. Removes
    /// delivered events past [`KEPT_FOR`] every [`CLEAN_EVERY`]. A failed pass is logged and tried again on the next.
    pub async fn run(&self, poll: Duration, stop: impl Future<Output = ()>) {
        tokio::pin!(stop);
        let mut woken = recorded().subscribe();
        let mut cleaned: Option<Instant> = None;
        loop {
            // Each failing outbox is logged where it fails.
            let _ = self.deliver_due().await;
            // A failed removal waits for the next hour too, rather than running on every poll.
            if cleaned.is_none_or(|at| at.elapsed() >= CLEAN_EVERY) {
                cleaned = Some(Instant::now());
                if let Err(error) = self.remove_delivered(KEPT_FOR).await {
                    tracing::error!(
                        error = described(&*error),
                        "removing delivered events failed"
                    );
                }
            }
            tokio::select! {
                () = &mut stop => return,
                () = tokio::time::sleep(poll) => {}
                _ = woken.changed() => {}
            }
        }
    }

    /// One pass over every outbox: each due event goes to every consumer that wants it, and is settled. An outbox
    /// that fails is logged by name and does not hold up the others, and a pool with no connection for a consumer
    /// ends the pass, so the dispatcher does not compete with requests for connections it cannot have.
    ///
    /// # Errors
    /// Fails, after the pass, with the first outbox that could not be read or settled. A consumer's own failure is
    /// not an error here; it schedules a retry.
    pub async fn deliver_due(&self) -> Result<usize, Error> {
        let mut delivered = 0;
        let mut first_error = None;
        'pass: for producer in &self.producers {
            for outbox in [Outbox::Tenant, Outbox::Global] {
                let place = Place::new(producer, outbox);
                match self.deliver_from(&place).await {
                    Ok(pass) => {
                        delivered += pass.delivered;
                        if pass.starved {
                            break 'pass;
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            outbox = place.table,
                            error = described(&*error),
                            "event delivery failed"
                        );
                        first_error.get_or_insert(error);
                    }
                }
            }
        }
        first_error.map_or(Ok(delivered), Err)
    }

    /// Delivers one outbox's due events, and commits what it has settled and charged when the pool has no
    /// connection for a consumer or the pass reaches [`PASS_LIMIT`], leaving the rest for the next pass.
    async fn deliver_from(&self, place: &Place<'_>) -> Result<Pass, Error> {
        let started = Instant::now();
        let mut client = self.pool.get().await?;
        let transaction = DeliveryTransaction::begin(&mut client).await?;
        let rows = due(&transaction, place).await?;
        let mut pass = Pass::default();
        for row in &rows {
            pass.count(self.deliver_one(&transaction, place, row).await?);
            if pass.starved || started.elapsed() >= PASS_LIMIT {
                break;
            }
        }
        transaction.commit().await?;
        Ok(pass)
    }

    /// Hands one due event over, then settles it, or schedules its retry.
    async fn deliver_one(
        &self,
        transaction: &Transaction<'_>,
        place: &Place<'_>,
        row: &tokio_postgres::Row,
    ) -> Result<Handed, Error> {
        let event = read_event(row, place.producer)?;
        let (failed, starved) = self.hand_over(&event).await;
        if !failed.is_empty() {
            retry_later(transaction, place, &event, &failed).await?;
        } else if !starved {
            settle(transaction, place, &event).await?;
        }
        Ok(match (starved, failed.is_empty()) {
            (true, _) => Handed::Starved,
            (false, true) => Handed::Delivered,
            (false, false) => Handed::Retried,
        })
    }

    /// Gives the event to each consumer that wants it, and returns those that failed, and whether the pool ran
    /// out of connections before every consumer had one. A shortage is no consumer's failure, so it charges only
    /// the failures that came before it.
    async fn hand_over(&self, event: &Event) -> (Vec<&dyn Consumer>, bool) {
        let mut failed = Vec::new();
        for consumer in self
            .consumers
            .iter()
            .filter(|consumer| consumer.wants(event))
        {
            let mut client = match self.pool.get().await {
                Ok(client) => client,
                Err(error) => {
                    tracing::warn!(
                        error = described(&error),
                        "no connection for a consumer; the pass ends here"
                    );
                    return (failed, true);
                }
            };
            let applied = tokio::time::timeout(
                self.apply_timeout,
                apply_once(&mut client, consumer.as_ref(), event),
            )
            .await
            .unwrap_or_else(|_| Err("the consumer did not answer in time".into()));
            if let Err(error) = applied {
                tracing::warn!(
                    event_id = event.id,
                    event_type = event.kind,
                    consumer = consumer.schema(),
                    error = described(&*error),
                    "event not applied"
                );
                failed.push(consumer.as_ref());
            }
        }
        (failed, false)
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
    place: &Place<'_>,
) -> Result<Vec<tokio_postgres::Row>, tokio_postgres::Error> {
    let table = &place.table;
    let tenant = match place.outbox {
        Outbox::Tenant => "tenant_id",
        Outbox::Global => "NULL::bigint",
    };
    transaction
        .query(
            &format!(
                "SELECT id::text, {tenant}, event_type, event_version, occurred_at, payload::text FROM {table}
                 WHERE delivered_at IS NULL AND due_at <= now() AND (keep OR attempts < $1)
                 ORDER BY due_at LIMIT $2 FOR UPDATE SKIP LOCKED"
            ),
            &[&MAX_ATTEMPTS, &BATCH],
        )
        .await
}

/// An outbox row as an event.
fn read_event(row: &tokio_postgres::Row, producer: &str) -> Result<Event, Error> {
    Ok(Event {
        id: row.try_get(0)?,
        producer: producer.to_string(),
        tenant: row.try_get(1)?,
        kind: row.try_get(2)?,
        version: row.try_get(3)?,
        occurred_at: row.try_get(4)?,
        payload: row.try_get(5)?,
    })
}

/// Marks an event delivered to every consumer that wants it, at the moment it is, not when the pass began.
async fn settle(
    transaction: &Transaction<'_>,
    place: &Place<'_>,
    event: &Event,
) -> Result<(), Error> {
    transaction
        .execute(
            &format!(
                "UPDATE {} SET delivered_at = clock_timestamp() WHERE {}",
                place.table,
                place.outbox.row()
            ),
            &[&event.id, &event.tenant],
        )
        .await?;
    Ok(())
}

/// Applies an event in the consumer's own transaction, for the event's tenant when it has one, which also records
/// it in the consumer's inbox, so an event already applied is skipped.
async fn apply_once(
    client: &mut Object,
    consumer: &dyn Consumer,
    event: &Event,
) -> Result<(), Error> {
    let schema = consumer.schema();
    let transaction = ModuleTransaction::begin(client, schema).await?;
    if let Some(tenant) = event.tenant {
        let transaction = transaction.for_tenant(tenant).await?;
        let inbox = format!(
            r#"INSERT INTO "{schema}".inbox (tenant_id, id) VALUES ($2, $1::text::uuid) ON CONFLICT DO NOTHING"#
        );
        once(&transaction, &inbox, &[&event.id, &tenant], consumer, event).await?;
        transaction.commit().await?;
    } else {
        let inbox = format!(
            r#"INSERT INTO "{schema}".inbox_global (id) VALUES ($1::text::uuid) ON CONFLICT DO NOTHING"#
        );
        once(&transaction, &inbox, &[&event.id], consumer, event).await?;
        transaction.commit().await?;
    }
    Ok(())
}

/// Records the event in the consumer's inbox with `inbox`, and applies it unless the inbox already held it.
async fn once(
    transaction: &Transaction<'_>,
    inbox: &str,
    values: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    consumer: &dyn Consumer,
    event: &Event,
) -> Result<(), Error> {
    if transaction.execute(inbox, values).await? == 1 {
        consumer.apply(transaction, event).await?;
    }
    Ok(())
}

/// Schedules the next attempt, from the moment of the failure, with exponential backoff and jitter capped at an
/// hour; the exponent stops growing well before the interval could overflow. Once an event has failed
/// [`MAX_ATTEMPTS`] times, it is marked kept if a never-lose consumer is among those still failing, and an error
/// names it and each failing consumer: one that is not kept stops there, and a kept one goes on.
async fn retry_later(
    transaction: &Transaction<'_>,
    place: &Place<'_>,
    event: &Event,
    failed: &[&dyn Consumer],
) -> Result<(), Error> {
    // Marked kept only when a never-lose consumer is still failing as the event runs out of attempts, so one that
    // failed once and recovered does not keep an event another consumer fails.
    let never_lose = failed.iter().any(|consumer| consumer.never_lose());
    let row = transaction
        .query_one(
            &format!(
                "UPDATE {} SET attempts = attempts + 1,
                   keep = keep OR ($3 AND attempts + 1 >= $4),
                   due_at = clock_timestamp() + least(
                     interval '1 hour',
                     interval '1 second' * power(2, least(attempts, 13)) * (0.5 + random() / 2))
                 WHERE {} RETURNING attempts, keep",
                place.table,
                place.outbox.row()
            ),
            &[&event.id, &event.tenant, &never_lose, &MAX_ATTEMPTS],
        )
        .await?;
    let (attempts, keep): (i64, bool) = (row.try_get(0)?, row.try_get(1)?);
    if attempts == MAX_ATTEMPTS {
        let what = if keep {
            "event keeps failing"
        } else {
            "event stopped retrying"
        };
        for consumer in failed {
            tracing::error!(
                event_id = event.id,
                event_type = event.kind,
                consumer = consumer.schema(),
                attempts,
                "{what}"
            );
        }
    }
    Ok(())
}
