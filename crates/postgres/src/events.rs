//! Domain events through a transactional outbox (ADR-0043). A module records an event in its own outbox in the
//! transaction that makes the change, and the dispatcher hands it to every consumer that wants it, at least once,
//! while each consumer's inbox makes its effect happen once.

use std::fmt::Write as _;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use std::sync::OnceLock;

use deadpool_postgres::{Object, Pool, Transaction};
use tokio::sync::watch;

use crate::connection::Error;
use crate::roles::DELIVERY_ROLE;
use crate::scope::{DeliveryTransaction, ModuleTransaction, TenantTransaction};

/// A module's outboxes, as one of its migrations.
pub const OUTBOX: &str = include_str!("../migrations/outbox.sql");
/// A consuming module's inboxes, as one of its migrations.
pub const INBOX: &str = include_str!("../migrations/inbox.sql");
/// The delivery role's right to remove a consuming module's inbox IDs past their use (ADR-0080), as one of its
/// migrations after [`INBOX`].
pub const INBOX_DELIVERY: &str = include_str!("../migrations/inbox_delivery.sql");
/// After this many failed attempts an event stops, unless it is kept, and its failure is logged as an error either
/// way, so operations can alert on it (#27).
pub const MAX_ATTEMPTS: i64 = 10;
/// How long a consumer may take to apply one event before it counts as failed, so one stuck on an outside call
/// cannot stop delivery.
const APPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a delivered event stays before it is deleted. The outbox delivers; it is no history.
pub const KEPT_FOR: Duration = Duration::from_hours(7 * 24);
/// How long an event that stopped retrying stays after its last attempt, to be found and replayed, before it is
/// deleted (ADR-0080). One marked never to be lost is never deleted undelivered.
pub const STOPPED_KEPT_FOR: Duration = Duration::from_hours(30 * 24);
/// How long an inbox keeps an applied event's ID at least, and after that only while an outbox still holds the
/// event, which could then come again (ADR-0080).
pub const APPLIED_KEPT_FOR: Duration = Duration::from_hours(30 * 24);
/// How often the dispatcher removes the rows past their use.
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

/// An event to record: a fact in the past tense, such as `identity.logged_in`, its version, and a JSON payload of
/// IDs and facts, never personal details. Its type is written in code, so the outbox is its check: a type other
/// than `module.fact` in lowercase letters, digits, and underscores, or a payload that is not a JSON object of at
/// most 64 KiB, is refused there (ADR-0060).
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
    /// How long a pass hands out events before it commits and leaves the rest for the next.
    pass_limit: Duration,
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

    /// Matches an outbox row `o` to the inbox row `i` its event left, by the outbox's whole key, so the lookup uses it.
    fn holds(self) -> &'static str {
        match self {
            Self::Tenant => "o.tenant_id = i.tenant_id AND o.id = i.id",
            Self::Global => "o.id = i.id",
        }
    }

    /// The inbox that holds the IDs of the events this kind of outbox delivers.
    fn inbox(self) -> &'static str {
        match self {
            Self::Tenant => "inbox",
            Self::Global => "inbox_global",
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
    /// Fails if the database cannot be reached, or a consumer has inboxes that lack what [`INBOX_DELIVERY`] grants:
    /// the delivery role's use of the schema, its rights to read and delete them, and its policy on a tenant inbox,
    /// so the role could never remove their IDs past their use. A schema not yet migrated is left to the checks
    /// that run before serving.
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
        for consumer in &consumers {
            let schema = consumer.schema();
            let cleanable: bool = client
                .query_one(
                    "SELECT coalesce(bool_and(
                       pg_catalog.has_schema_privilege($2, n.oid, 'USAGE')
                       AND pg_catalog.has_table_privilege($2, c.oid, 'SELECT')
                       AND pg_catalog.has_table_privilege($2, c.oid, 'DELETE')
                       AND (NOT c.relrowsecurity OR EXISTS (
                         SELECT FROM pg_catalog.pg_policy p JOIN pg_catalog.pg_roles r ON r.oid = any(p.polroles)
                         WHERE p.polrelid = c.oid AND r.rolname = $2))), true)
                     FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                     WHERE n.nspname = $1 AND c.relname IN ('inbox', 'inbox_global') AND c.relkind = 'r'",
                    &[&schema, &DELIVERY_ROLE],
                )
                .await?
                .try_get(0)?;
            if !cleanable {
                return Err(format!(
                    "{schema} consumes events, but its migrations lack INBOX_DELIVERY"
                )
                .into());
            }
        }
        Ok(Self {
            pool,
            producers,
            consumers,
            apply_timeout: APPLY_TIMEOUT,
            pass_limit: PASS_LIMIT,
        })
    }

    /// The same dispatcher, ending each pass after `limit`, so tests need not wait out the real one.
    #[doc(hidden)]
    #[must_use]
    pub fn with_pass_limit(self, limit: Duration) -> Self {
        Self {
            pass_limit: limit,
            ..self
        }
    }

    /// Makes an event that stopped retrying due again, with its attempts cleared, once the consumer that failed
    /// it is fixed (ADR-0043). `producer` is the schema of the module that recorded it, which must be one of this
    /// dispatcher's producers, so no other name reaches the SQL. It acts as the delivery role, which may settle
    /// every tenant's events, and returns the outbox that held the event, or none if neither did, as when it was
    /// removed past its use: delivered over [`KEPT_FOR`] ago, or stopped over [`STOPPED_KEPT_FOR`] ago (ADR-0080).
    ///
    /// # Errors
    /// Fails if `producer` records no events here, `id` is not a UUID, or the database fails.
    pub async fn replay(&self, producer: &str, id: &str) -> Result<Option<&'static str>, Error> {
        let Some(producer) = self.producers.iter().find(|known| *known == producer) else {
            return Err(format!("{producer:?} records no events here").into());
        };
        let mut client = self.pool.get().await?;
        let transaction = DeliveryTransaction::begin(&mut client).await?;
        let mut found = None;
        for outbox in [Outbox::Tenant, Outbox::Global] {
            let place = Place::new(producer, outbox);
            let replayed = transaction
                .execute(
                    &format!(
                        "UPDATE {} SET attempts = 0, due_at = clock_timestamp() WHERE id = $1::text::uuid",
                        place.table
                    ),
                    &[&id],
                )
                .await?;
            if replayed > 0 {
                found = Some(outbox.table());
                break;
            }
        }
        transaction.commit().await?;
        Ok(found)
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

    /// Delivers until `stop` finishes: every `poll`, and at once when this process records an event. Removes the
    /// rows past their use with [`Self::remove_spent`] once an hour, logging how many when it removed any. A failed
    /// pass is logged and tried again on the next.
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
                match self.remove_spent().await {
                    Ok(removed) if removed != Removed::default() => tracing::info!(
                        delivered = removed.delivered,
                        stopped = removed.stopped,
                        applied = removed.applied,
                        "removed event rows past their use"
                    ),
                    Ok(_) => {}
                    Err(error) => tracing::error!(
                        error = described(&*error),
                        "removing event rows past their use failed"
                    ),
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
        let deadline = Instant::now() + self.pass_limit;
        let mut client = self.pool.get().await?;
        let transaction = DeliveryTransaction::begin(&mut client).await?;
        let rows = due(&transaction, place).await?;
        let mut pass = Pass::default();
        for row in &rows {
            pass.count(self.deliver_one(&transaction, place, row, deadline).await?);
            if pass.starved || Instant::now() >= deadline {
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
        deadline: Instant,
    ) -> Result<Handed, Error> {
        let event = read_event(row, place.producer)?;
        let (failed, starved, owed) = self.hand_over(&event, deadline).await;
        if !failed.is_empty() {
            retry_later(transaction, place, &event, &failed, owed).await?;
        } else if !starved {
            settle(transaction, place, &event).await?;
        }
        Ok(match (starved, failed.is_empty()) {
            (true, _) => Handed::Starved,
            (false, true) => Handed::Delivered,
            (false, false) => Handed::Retried,
        })
    }

    /// Gives the event to each consumer that wants it, and returns those that failed, and whether the pass ended
    /// before every consumer had it: the pool ran out of connections, or the pass reached its deadline before a
    /// later consumer. Neither is a consumer's failure, so it charges only the failures that came before it. The
    /// first consumer always runs, so every pass moves on, and no pass outlasts its deadline by more than one
    /// consumer's time, which the delivery transaction's limits cover (ADR-0060). It also says whether a consumer
    /// that must never lose an event was left without it, so the event is kept as if that consumer had failed.
    async fn hand_over(
        &self,
        event: &Event,
        deadline: Instant,
    ) -> (Vec<&dyn Consumer>, bool, bool) {
        let mut failed = Vec::new();
        let wanting: Vec<&Arc<dyn Consumer>> = self
            .consumers
            .iter()
            .filter(|consumer| consumer.wants(event))
            .collect();
        let owed = |from: usize| wanting[from..].iter().any(|consumer| consumer.never_lose());
        for (index, consumer) in wanting.iter().copied().enumerate() {
            if index > 0 && Instant::now() >= deadline {
                tracing::info!(
                    event_id = event.id,
                    "the pass reached its limit; the event waits for the next"
                );
                return (failed, true, owed(index));
            }
            let mut client = match self.pool.get().await {
                Ok(client) => client,
                Err(error) => {
                    tracing::warn!(
                        error = described(&error),
                        "no connection for a consumer; the pass ends here"
                    );
                    return (failed, true, owed(index));
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
        (failed, false, false)
    }

    /// Deletes the rows past their use (ADR-0080), and says how many of each: events delivered over [`KEPT_FOR`]
    /// ago and events that stopped retrying over [`STOPPED_KEPT_FOR`] ago, in one transaction, then inbox IDs
    /// applied over [`APPLIED_KEPT_FOR`] ago whose event no outbox of their kind still holds, in batches that each
    /// commit, so a large inbox never holds the outboxes' cleanup back or outlasts a statement's limit. Every outbox
    /// the database holds counts, not just this dispatcher's, so an instance of an older build never drops the ID
    /// of an event a newer one still delivers.
    ///
    /// # Errors
    /// Fails if a step cannot finish; the steps before it stay done.
    pub async fn remove_spent(&self) -> Result<Removed, Error> {
        let mut removed = Removed::default();
        let mut client = self.pool.get().await?;
        let transaction = DeliveryTransaction::begin(&mut client).await?;
        for outbox in [Outbox::Tenant, Outbox::Global] {
            for producer in &self.producers {
                let table = format!(r#""{producer}".{}"#, outbox.table());
                let (delivered, stopped) = remove_from_outbox(&transaction, &table).await?;
                removed.delivered += delivered;
                removed.stopped += stopped;
            }
        }
        let outboxes = every_outbox(&transaction).await?;
        transaction.commit().await?;
        // An outbox this dispatcher delivers that is not found as a shared one has lost what marks it, so the
        // cleanup stops rather than drop IDs its events still need.
        if let Some(lost) = self
            .producers
            .iter()
            .find(|producer| !outboxes.contains(producer))
        {
            return Err(format!("{lost}'s outbox lacks the delivery role's policy").into());
        }
        removed.applied = self.remove_applied(&mut client, &outboxes).await?;
        Ok(removed)
    }

    /// Removes from every consumer's inboxes the IDs past their use whose event none of `outboxes` holds.
    async fn remove_applied(&self, client: &mut Object, outboxes: &[String]) -> Result<u64, Error> {
        let (mut removed, mut batches) = (0, APPLIED_BATCHES);
        for outbox in [Outbox::Tenant, Outbox::Global] {
            let held = outboxes.iter().fold(String::new(), |mut held, producer| {
                let _ = write!(
                    held,
                    " AND NOT EXISTS (SELECT FROM {}.{} AS o WHERE {})",
                    quoted(producer),
                    outbox.table(),
                    outbox.holds()
                );
                held
            });
            for consumer in &self.consumers {
                let inbox = format!(r#""{}".{}"#, consumer.schema(), outbox.inbox());
                removed += remove_from_inbox(client, &inbox, &held, &mut batches).await?;
            }
        }
        Ok(removed)
    }
}

/// How many inbox IDs one statement removes at most.
const APPLIED_BATCH: i64 = 1000;
/// How many batches one pass runs at most, across every inbox, so delivery, which waits for the cleanup, waits for
/// no more than these; any left wait for the next pass.
const APPLIED_BATCHES: usize = 20;

/// A schema's name as an identifier SQL can hold whatever it is, quoted as Postgres quotes one.
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Every schema in the database with the shared outbox, whichever dispatcher knows it. The shared outbox is the one [`OUTBOX`] made: a permanent table that holds the delivery role's own
/// policy. Any other table of that name, such as one in `public` or a session's temporary one, is no module's
/// outbox, and a shared one the role can no longer read fails the cleanup rather than being passed over.
async fn every_outbox(transaction: &Transaction<'_>) -> Result<Vec<String>, tokio_postgres::Error> {
    transaction
        .query(
            "SELECT n.nspname::text FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE c.relname = 'outbox' AND c.relkind = 'r' AND c.relpersistence = 'p'
               AND EXISTS (
                 SELECT FROM pg_catalog.pg_policy p JOIN pg_catalog.pg_roles r ON r.oid = any(p.polroles)
                 WHERE p.polrelid = c.oid AND p.polname = 'delivery' AND r.rolname = current_user)
             ORDER BY 1",
            &[],
        )
        .await?
        .iter()
        .map(|row| row.try_get(0))
        .collect()
}

/// Removes the IDs applied over [`APPLIED_KEPT_FOR`] ago from one inbox, unless `held` finds their event in an
/// outbox, a batch to a transaction, while the pass has `batches` left that removed some, and says how many.
async fn remove_from_inbox(
    client: &mut Object,
    inbox: &str,
    held: &str,
    batches: &mut usize,
) -> Result<u64, Error> {
    let mut removed = 0;
    while *batches > 0 {
        let transaction = DeliveryTransaction::begin(client).await?;
        let batch = transaction
            .execute(
                &format!(
                    "DELETE FROM {inbox} WHERE ctid = any(array(
                       SELECT i.ctid FROM {inbox} AS i
                       WHERE i.applied_at < now() - make_interval(secs => $1){held} LIMIT $2))"
                ),
                &[&APPLIED_KEPT_FOR.as_secs_f64(), &APPLIED_BATCH],
            )
            .await?;
        transaction.commit().await?;
        // A batch that found nothing means none is left, and costs the pass none of its batches: each inbox adds at
        // most that one empty statement.
        if batch == 0 {
            break;
        }
        removed += batch;
        *batches -= 1;
    }
    Ok(removed)
}

/// Removes from one outbox the events delivered over [`KEPT_FOR`] ago, and those that stopped retrying over
/// [`STOPPED_KEPT_FOR`] ago and are not kept, and says how many of each.
async fn remove_from_outbox(
    transaction: &Transaction<'_>,
    table: &str,
) -> Result<(u64, u64), tokio_postgres::Error> {
    let delivered = transaction
        .execute(
            &format!("DELETE FROM {table} WHERE delivered_at < now() - make_interval(secs => $1)"),
            &[&KEPT_FOR.as_secs_f64()],
        )
        .await?;
    let stopped = transaction
        .execute(
            &format!(
                "DELETE FROM {table} WHERE delivered_at IS NULL AND NOT keep AND attempts >= $2
                   AND due_at < now() - make_interval(secs => $1)"
            ),
            &[&STOPPED_KEPT_FOR.as_secs_f64(), &MAX_ATTEMPTS],
        )
        .await?;
    Ok((delivered, stopped))
}

/// How many rows past their use one cleanup removed, of each kind (ADR-0080).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Removed {
    pub delivered: u64,
    pub stopped: u64,
    pub applied: u64,
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
/// [`MAX_ATTEMPTS`] times, it is marked kept if a never-lose consumer is among those still failing or not reached
/// (`owed`), and an error
/// names it and each failing consumer: one that is not kept stops there, and a kept one goes on.
async fn retry_later(
    transaction: &Transaction<'_>,
    place: &Place<'_>,
    event: &Event,
    failed: &[&dyn Consumer],
    owed: bool,
) -> Result<(), Error> {
    // Marked kept only when a never-lose consumer is still failing, or was not reached, as the event runs out of
    // attempts, so one that failed once and recovered does not keep an event another consumer fails.
    let never_lose = owed || failed.iter().any(|consumer| consumer.never_lose());
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
                alert = "event_stopped",
                "{what}"
            );
        }
    }
    Ok(())
}
