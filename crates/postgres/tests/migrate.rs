//! Runs the migration runner against a real Postgres.

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::fresh_database;
use tetsu_postgres::{connect, migrate};

struct Fake(&'static [Migration]);

impl Module for Fake {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn storage(&self) -> Option<Storage> {
        Some(Storage {
            schema: "fake",
            migrations: self.0,
        })
    }
}

const FIRST: Migration = Migration {
    version: 1,
    sql: "CREATE TABLE fake.a (id int)",
};
const TWO_STATEMENTS: Migration = Migration {
    version: 2,
    sql: "-- tetsu:no-transaction\nCREATE INDEX CONCURRENTLY x ON fake.a (id); SELECT 1",
};
const DUPLICATES: Migration = Migration {
    version: 2,
    sql: "INSERT INTO fake.a VALUES (1), (1)",
};
const UNIQUE: Migration = Migration {
    version: 3,
    sql: "-- tetsu:no-transaction\nCREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS a_unique ON fake.a (id)",
};
const SECOND: Migration = Migration {
    version: 2,
    sql: "CREATE TABLE fake.b (id int)",
};
const CHANGED: Migration = Migration {
    version: 1,
    sql: "CREATE TABLE fake.other (id int)",
};
const BAD: Migration = Migration {
    version: 1,
    sql: "CREATE TABLE fake.c (id int); SELECT nope",
};
const CONCURRENT: Migration = Migration {
    version: 2,
    sql: "-- tetsu:no-transaction\nCREATE INDEX CONCURRENTLY a_id ON fake.a (id)",
};

async fn table_exists(
    settings: &tetsu_postgres::Settings,
    name: &str,
) -> Result<bool, tetsu_postgres::Error> {
    let client = connect(settings).await?;
    let row = client
        .query_one("SELECT to_regclass($1)::text AS found", &[&name])
        .await?;
    Ok(row.try_get::<_, Option<String>>("found")?.is_some())
}

#[tokio::test]
async fn applies_migrations_and_skips_them_the_second_time() {
    let settings = fresh_database().await.expect("a test database");
    let module = Fake(&[FIRST, SECOND]);
    migrate(&settings, &[&module]).await.expect("first run");
    assert!(
        table_exists(&settings, "fake.a")
            .await
            .expect("checks the table")
    );
    assert!(
        table_exists(&settings, "fake.b")
            .await
            .expect("checks the table")
    );
    migrate(&settings, &[&module])
        .await
        .expect("second run applies nothing, so it does not fail");
}

#[tokio::test]
async fn applies_only_what_is_new() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST])]).await.expect("first");
    assert!(
        !table_exists(&settings, "fake.b")
            .await
            .expect("checks the table")
    );
    migrate(&settings, &[&Fake(&[FIRST, SECOND])])
        .await
        .expect("second");
    assert!(
        table_exists(&settings, "fake.b")
            .await
            .expect("checks the table")
    );
}

#[tokio::test]
async fn refuses_an_applied_file_that_changed() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST])]).await.expect("first");
    let error = migrate(&settings, &[&Fake(&[CHANGED])])
        .await
        .expect_err("must refuse");
    assert!(error.to_string().contains("version 1"), "{error}");
}

#[tokio::test]
async fn a_failed_file_rolls_back_and_is_not_recorded() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[BAD])])
        .await
        .expect_err("must fail");
    assert!(
        !table_exists(&settings, "fake.c")
            .await
            .expect("checks the table")
    );
    migrate(&settings, &[&Fake(&[FIRST])])
        .await
        .expect("version 1 was not recorded");
}

#[tokio::test]
async fn a_file_can_opt_out_of_the_transaction() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, CONCURRENT])])
        .await
        .expect("runs outside a transaction");
    assert!(
        table_exists(&settings, "fake.a_id")
            .await
            .expect("checks the table")
    );
}

#[tokio::test]
async fn a_change_and_its_record_commit_together() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[])])
        .await
        .expect("creates the tracking table");
    let client = connect(&settings).await.expect("connects");
    client
        .batch_execute(
            "CREATE FUNCTION fake.refuse() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE 'no'; END $$;
             CREATE TRIGGER refuse BEFORE INSERT ON fake.migrations FOR EACH ROW EXECUTE FUNCTION fake.refuse()",
        )
        .await
        .expect("blocks recording");
    migrate(&settings, &[&Fake(&[FIRST])])
        .await
        .expect_err("recording fails");
    assert!(
        !table_exists(&settings, "fake.a")
            .await
            .expect("checks the table")
    );
}

#[tokio::test]
async fn a_file_outside_a_transaction_holds_one_statement() {
    let settings = fresh_database().await.expect("a test database");
    let error = migrate(&settings, &[&Fake(&[FIRST, TWO_STATEMENTS])])
        .await
        .expect_err("must refuse");
    assert!(error.to_string().contains("one statement"), "{error}");
}

#[tokio::test]
async fn a_rerun_refuses_an_invalid_index_left_behind() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, DUPLICATES])])
        .await
        .expect("seeds duplicates");
    migrate(&settings, &[&Fake(&[FIRST, DUPLICATES, UNIQUE])])
        .await
        .expect_err("the index build fails");
    let client = connect(&settings).await.expect("connects");
    client
        .batch_execute("DELETE FROM fake.a")
        .await
        .expect("clears the duplicates");
    let error = migrate(&settings, &[&Fake(&[FIRST, DUPLICATES, UNIQUE])])
        .await
        .expect_err("must refuse");
    assert!(error.to_string().contains("invalid index"), "{error}");
    let recorded = client
        .query_one(
            "SELECT count(*) AS n FROM fake.migrations WHERE version = 3",
            &[],
        )
        .await
        .expect("counts");
    assert_eq!(recorded.try_get::<_, i64>("n").expect("maps"), 0);
}

#[tokio::test]
async fn modules_with_no_schema_are_skipped() {
    struct NoSchema;
    impl Module for NoSchema {
        fn name(&self) -> &'static str {
            "none"
        }
    }
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&NoSchema])
        .await
        .expect("nothing to do");
}

#[tokio::test]
async fn two_runners_at_once_both_finish() {
    let settings = fresh_database().await.expect("a test database");
    let module = Fake(&[FIRST, SECOND]);
    let modules: [&dyn Module; 1] = [&module];
    let (a, b) = tokio::join!(migrate(&settings, &modules), migrate(&settings, &modules));
    a.expect("first runner");
    b.expect("second runner waits for the lock");
}

#[tokio::test]
async fn gives_up_when_the_lock_stays_held() {
    let settings = fresh_database().await.expect("a test database");
    let holder = connect(&settings).await.expect("connects");
    holder
        .execute(
            "SELECT pg_advisory_lock($1)",
            &[&tetsu_postgres::MIGRATION_LOCK],
        )
        .await
        .expect("takes the lock");
    let error =
        tetsu_postgres::migrate_with_wait(&settings, &[], std::time::Duration::from_millis(300))
            .await
            .expect_err("must give up");
    assert!(error.to_string().contains("lock"), "{error}");
}

#[tokio::test]
async fn refuses_a_database_that_is_not_utf8() {
    let admin_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is set");
    let admin = connect(&tetsu_postgres::Settings::from_values(
        &admin_url,
        Some("disable"),
    ))
    .await
    .expect("connects");
    let name = format!("latin1_{}", std::process::id());
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name}"))
        .await
        .expect("drops a leftover database");
    admin
        .batch_execute(&format!(
            "CREATE DATABASE {name} ENCODING 'LATIN1' TEMPLATE template0 LC_COLLATE 'C' LC_CTYPE 'C'"
        ))
        .await
        .expect("creates a LATIN1 database");
    let (base, _) = admin_url.rsplit_once('/').expect("names a database");
    let settings =
        tetsu_postgres::Settings::from_values(&format!("{base}/{name}"), Some("disable"));

    let error = migrate(&settings, &[&Fake(&[FIRST])]).await.unwrap_err();
    assert!(
        error.to_string().contains("must use UTF8, not LATIN1"),
        "{error}"
    );
    assert!(
        !table_exists(&settings, "fake.migrations")
            .await
            .expect("checks")
    );
}
