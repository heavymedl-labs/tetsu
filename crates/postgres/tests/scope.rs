//! Tenant isolation through the scoped transactions, logged in as the application role (ADR-0042).

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{ModuleTransaction, Pool, TenantTransaction, migrate, pool};
use tokio_postgres::error::SqlState;

/// A tenant-owned table of people, as a customer module would keep them: one email per tenant, never across.
struct People;

const PEOPLE: Migration = Migration {
    version: 1,
    sql: include_str!("people.sql"),
};

impl Module for People {
    fn name(&self) -> &'static str {
        "people"
    }
    fn storage(&self) -> Option<Storage> {
        Some(Storage {
            schema: "people",
            migrations: &[PEOPLE],
        })
    }
}

const ANVIL: i64 = 1;
const JUNIPER: i64 = 2;
const LEE: &str = "lee.park@example.com";

#[cfg(test)]
mod support {
    use super::{People, Pool, TenantTransaction, TestDatabase, fresh_database, migrate, pool};

    /// A migrated database and a pool logged in as the application role. Keep both until the test ends.
    pub async fn app_pool() -> (Pool, TestDatabase) {
        let database = fresh_database().await.expect("a test database");
        migrate(&database, &[&People]).await.expect("migrates");
        (pool(&database.as_app()).expect("pool"), database)
    }

    pub async fn add(pool: &Pool, tenant: i64, email: &str) -> Result<u64, tokio_postgres::Error> {
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "people", tenant).await?;
        let added = transaction
            .execute(
                "INSERT INTO people.people (tenant_id, email) VALUES ($1, $2)",
                &[&tenant, &email],
            )
            .await?;
        transaction.commit().await?;
        Ok(added)
    }

    pub async fn count(pool: &Pool, tenant: i64, query: &str) -> i64 {
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "people", tenant)
            .await
            .expect("begins");
        transaction
            .query_one(query, &[])
            .await
            .expect("counts")
            .try_get(0)
            .expect("a count")
    }

    pub async fn session(client: &tokio_postgres::Client) -> (String, String, String, String) {
        let row = client
            .query_one(
                "SELECT current_user::text, current_setting('app.tenant_id', true),
                        current_setting('statement_timeout'), current_setting('lock_timeout')",
                &[],
            )
            .await
            .expect("reads the session");
        let text = |column: usize| row.try_get::<_, String>(column).expect("text");
        (
            text(0),
            row.try_get::<_, Option<String>>(1)
                .expect("text")
                .unwrap_or_default(),
            text(2),
            text(3),
        )
    }
}

use support::{add, app_pool, count, session};

/// #8's case: Lee Park is a customer of both businesses with one email, and neither sees the other's record.
#[tokio::test]
async fn two_tenants_hold_one_email_and_neither_learns_of_the_other() {
    let (pool, _database) = app_pool().await;
    assert_eq!(add(&pool, ANVIL, LEE).await.expect("adds at Anvil Row"), 1);
    assert_eq!(
        add(&pool, JUNIPER, LEE)
            .await
            .expect("adds at Juniper Lane"),
        1
    );
    assert_eq!(
        count(&pool, ANVIL, "SELECT count(*) FROM people.people").await,
        1
    );
    assert_eq!(
        count(
            &pool,
            ANVIL,
            "SELECT count(*) FROM people.people WHERE tenant_id = 2"
        )
        .await,
        0
    );
    let duplicate = add(&pool, ANVIL, LEE)
        .await
        .expect_err("one email per tenant");
    assert_eq!(duplicate.code(), Some(&SqlState::UNIQUE_VIOLATION));
}

#[tokio::test]
async fn a_tenant_cannot_write_another_tenant_s_rows() {
    let (pool, _database) = app_pool().await;
    add(&pool, JUNIPER, LEE)
        .await
        .expect("adds at Juniper Lane");
    let mut client = pool.get().await.expect("a connection");
    let transaction = TenantTransaction::begin(&mut client, "people", ANVIL)
        .await
        .expect("begins");
    // Blocked updates and deletes report no rows rather than an error, so the counts are the test.
    let updated = transaction
        .execute("UPDATE people.people SET email = 'taken@example.com'", &[])
        .await
        .expect("updates nothing");
    let deleted = transaction
        .execute("DELETE FROM people.people", &[])
        .await
        .expect("deletes nothing");
    assert_eq!((updated, deleted), (0, 0));
    let refused = transaction
        .execute(
            "INSERT INTO people.people (tenant_id, email) VALUES ($1, $2)",
            &[&JUNIPER, &"planted@example.com"],
        )
        .await
        .expect_err("refuses a row for another tenant");
    assert_eq!(refused.code(), Some(&SqlState::INSUFFICIENT_PRIVILEGE));
}

#[tokio::test]
async fn without_a_tenant_tenant_owned_rows_are_out_of_reach() {
    let (pool, _database) = app_pool().await;
    add(&pool, ANVIL, LEE).await.expect("adds");
    let mut client = pool.get().await.expect("a connection");
    let transaction = ModuleTransaction::begin(&mut client, "people")
        .await
        .expect("begins");
    let seen: i64 = transaction
        .query_one("SELECT count(*) FROM people.people", &[])
        .await
        .expect("counts")
        .try_get(0)
        .expect("a count");
    assert_eq!(seen, 0);
}

#[tokio::test]
async fn the_app_role_reaches_nothing_without_a_module_role() {
    let (pool, _database) = app_pool().await;
    let client = pool.get().await.expect("a connection");
    let refused = client
        .query_one("SELECT count(*) FROM people.people", &[])
        .await
        .expect_err("holds no rights of its own");
    assert_eq!(refused.code(), Some(&SqlState::INSUFFICIENT_PRIVILEGE));
    for role in ["postgres", "tetsu_people_lookup"] {
        let refused = client
            .query_one("SELECT pg_catalog.set_config('role', $1, true)", &[&role])
            .await
            .expect_err("may act only as module roles");
        assert_eq!(
            refused.code(),
            Some(&SqlState::INSUFFICIENT_PRIVILEGE),
            "{role}"
        );
    }
}

#[tokio::test]
async fn a_transaction_acts_as_its_module_with_request_limits() {
    let (pool, _database) = app_pool().await;
    let mut client = pool.get().await.expect("a connection");
    let transaction = TenantTransaction::begin(&mut client, "people", ANVIL)
        .await
        .expect("begins");
    assert_eq!(
        session(transaction.client()).await,
        ("tetsu_people".into(), "1".into(), "5s".into(), "2s".into())
    );
}

/// The pool keeps session settings between checkouts, so nothing a transaction set may outlive it (#53).
#[tokio::test]
async fn nothing_a_transaction_sets_reaches_the_next_request() {
    let (pool, _database) = app_pool().await;
    let clean = (
        "tetsu_app".to_string(),
        String::new(),
        "5s".to_string(),
        "0".to_string(),
    );
    {
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "people", ANVIL)
            .await
            .expect("begins");
        transaction.commit().await.expect("commits");
        assert_eq!(session(&client).await, clean, "after a commit");
    }
    {
        let mut client = pool.get().await.expect("the same connection, reused");
        let transaction = TenantTransaction::begin(&mut client, "people", JUNIPER)
            .await
            .expect("begins");
        drop(transaction);
        assert_eq!(session(&client).await, clean, "after a rollback");
    }
    let client = pool.get().await.expect("the same connection, reused");
    assert_eq!(pool.status().size, 1, "one connection served every request");
    assert_eq!(session(&client).await, clean, "on the next checkout");
}

/// The probe hosted test 8 runs: the module's role sees no rows with no tenant set, and the catalog shows no
/// problem with a table shaped as ADR-0042 asks.
#[tokio::test]
async fn the_hosted_probe_finds_no_rows_without_a_tenant() {
    let (pool, database) = app_pool().await;
    add(&pool, 1, "lee.park@example.com").await.expect("adds");
    let mut client = tetsu_postgres::connect(&database).await.expect("connects");
    assert_eq!(
        tetsu_postgres::rows_seen_without_a_tenant(&mut client, &["people"])
            .await
            .expect("probes"),
        Vec::<String>::new()
    );
    assert_eq!(
        tetsu_postgres::isolation_violations(&mut client, &["people"])
            .await
            .expect("checks"),
        Vec::<String>::new()
    );
    // A tenant set for the checker's own sessions must not narrow what it counts.
    tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .batch_execute(
            "DO $$ BEGIN
               EXECUTE format('ALTER ROLE postgres IN DATABASE %I SET app.tenant_id = %L', current_database(), '1');
             END $$",
        )
        .await
        .expect("sets a tenant for the checker");
    let mut client = tetsu_postgres::connect(&database).await.expect("connects");
    assert_eq!(
        tetsu_postgres::rows_seen_without_a_tenant(&mut client, &["people"])
            .await
            .expect("probes despite the checker's tenant"),
        Vec::<String>::new()
    );
    client
        .batch_execute(
            "ALTER TABLE people.people NO FORCE ROW LEVEL SECURITY, DISABLE ROW LEVEL SECURITY",
        )
        .await
        .expect("breaks the table");
    assert_eq!(
        tetsu_postgres::rows_seen_without_a_tenant(&mut client, &["people"])
            .await
            .expect("probes"),
        ["people.people shows 1 rows with no tenant set"]
    );
}

#[tokio::test]
async fn a_module_transaction_keeps_what_it_commits() {
    let (pool, database) = app_pool().await;
    let mut client = pool.get().await.expect("a connection");
    let transaction = ModuleTransaction::begin(&mut client, "people")
        .await
        .expect("begins");
    transaction
        .execute("INSERT INTO people.notes VALUES (1)", &[])
        .await
        .expect("writes");
    transaction.commit().await.expect("commits");
    let kept: i64 = tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .query_one("SELECT count(*) FROM people.notes", &[])
        .await
        .expect("counts")
        .try_get(0)
        .expect("a count");
    assert_eq!(kept, 1);
}

#[tokio::test]
async fn module_schemas_are_found_by_their_records() {
    let (_pool, database) = app_pool().await;
    let client = tetsu_postgres::connect(&database).await.expect("connects");
    // Another tool's table of the same name is no module's record.
    client
        .batch_execute("CREATE TABLE public.migrations (id int)")
        .await
        .expect("makes a look-alike");
    assert_eq!(
        tetsu_postgres::module_schemas(&client)
            .await
            .expect("finds them"),
        ["people"]
    );
}

/// A tenant left on the session, however it got there, never reaches a module transaction.
#[tokio::test]
async fn a_module_transaction_ignores_a_tenant_left_on_the_session() {
    let (pool, _database) = app_pool().await;
    add(&pool, 1, "lee.park@example.com").await.expect("adds");
    let mut client = pool.get().await.expect("a connection");
    client
        .query_one(
            "SELECT pg_catalog.set_config('app.tenant_id', '1', false)",
            &[],
        )
        .await
        .expect("leaves a tenant on the session");
    let transaction = ModuleTransaction::begin(&mut client, "people")
        .await
        .expect("begins");
    let seen: i64 = transaction
        .query_one("SELECT count(*) FROM people.people", &[])
        .await
        .expect("counts")
        .try_get(0)
        .expect("a count");
    assert_eq!(seen, 0);
}

/// A request's transaction ends if it runs or sits idle too long, and the limits leave with it (ADR-0060).
#[tokio::test]
async fn a_transaction_has_limits_on_its_length_and_idling() {
    let (pool, _database) = app_pool().await;
    let mut client = pool.get().await.expect("a connection");
    let limits = async |client: &tokio_postgres::Client| {
        let row = client
            .query_one(
                "SELECT current_setting('transaction_timeout'),
                        current_setting('idle_in_transaction_session_timeout')",
                &[],
            )
            .await
            .expect("reads the limits");
        (
            row.try_get::<_, String>(0).expect("text"),
            row.try_get::<_, String>(1).expect("text"),
        )
    };
    let transaction = TenantTransaction::begin(&mut client, "people", ANVIL)
        .await
        .expect("begins");
    assert_eq!(
        limits(transaction.client()).await,
        ("15s".into(), "10s".into())
    );
    transaction.commit().await.expect("commits");
    let transaction = ModuleTransaction::begin(&mut client, "people")
        .await
        .expect("begins");
    assert_eq!(
        limits(transaction.client()).await,
        ("15s".into(), "10s".into())
    );
    drop(transaction);
    assert_eq!(limits(&client).await, ("0".into(), "0".into()));
}

/// Every scoped transaction reads committed rows afresh at each statement, whatever default the server or role
/// sets, since Identity's disable and every lock-then-change function rely on it (ADR-0059): under repeatable read
/// a statement after a lock wait would still miss what the other transaction committed.
#[tokio::test]
async fn every_scoped_transaction_reads_committed() {
    let (pool, _database) = app_pool().await;
    let mut client = pool.get().await.expect("a connection");
    client
        .batch_execute("SET default_transaction_isolation = 'repeatable read'")
        .await
        .expect("sets a stricter default");
    let transaction = ModuleTransaction::begin(&mut client, "people")
        .await
        .expect("begins");
    let isolation: String = transaction
        .query_one("SELECT current_setting('transaction_isolation')", &[])
        .await
        .expect("reads")
        .try_get(0)
        .expect("text");
    assert_eq!(isolation, "read committed");
}
