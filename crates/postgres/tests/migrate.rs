//! Runs the migration runner against a real Postgres.

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::{fresh_database, fresh_database_encoded};
use tetsu_postgres::{
    Behind, MAX_SCHEMA, MIGRATION_LOCK, behind_code, connect, migrate, migrate_with_wait, pool,
    require_supported_server,
};

struct Fake(&'static [Migration]);

/// A module with a chosen name and schema, for checks across modules.
struct Named(&'static str, &'static str, &'static [Migration]);

impl Module for Named {
    fn name(&self) -> &'static str {
        self.0
    }
    fn storage(&self) -> Option<Storage> {
        Some(Storage {
            schema: self.1,
            migrations: self.2,
        })
    }
}

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

const THIRD: Migration = Migration {
    version: 3,
    sql: "CREATE TABLE fake.d (id int)",
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

/// The fake module's storage, with `migrations`.
fn storage(migrations: &'static [Migration]) -> Storage {
    Storage {
        schema: "fake",
        migrations,
    }
}

/// What serving checks before it reads a module's tables, as the application role and writing nothing: a module
/// never migrated has its first file unapplied.
#[tokio::test]
async fn a_module_never_migrated_is_behind_its_code() {
    let database = fresh_database().await.expect("a test database");
    let app = pool(&database.as_app()).expect("a pool");
    let unmigrated = behind_code(&app, &[storage(&[FIRST])])
        .await
        .expect("checks");
    assert_eq!(
        unmigrated,
        Some(Behind::Unapplied {
            schema: "fake",
            version: 1
        })
    );
    let schemas = connect(&database)
        .await
        .expect("connects")
        .query_one(
            "SELECT count(*) FROM pg_catalog.pg_namespace WHERE nspname = 'fake'",
            &[],
        )
        .await
        .expect("counts")
        .try_get::<_, i64>(0)
        .expect("a count");
    assert_eq!(schemas, 0, "the check creates nothing");
}

/// Once migrated, the first module behind its code, by a file not yet applied or one changed since; none when the
/// database is up to date or ahead of the code, as after a rollback (ADR-0033).
#[tokio::test]
async fn serving_finds_a_database_behind_its_code() {
    let database = fresh_database().await.expect("a test database");
    let app = pool(&database.as_app()).expect("a pool");
    migrate(&database, &[&Fake(&[FIRST, SECOND])])
        .await
        .expect("migrates");
    for (code, found) in [
        (&[FIRST, SECOND][..], None),
        (&[FIRST], None),
        (
            &[FIRST, SECOND, UNIQUE],
            Some(Behind::Unapplied {
                schema: "fake",
                version: 3,
            }),
        ),
        (
            &[CHANGED],
            Some(Behind::Changed {
                schema: "fake",
                version: 1,
            }),
        ),
    ] {
        let checked = behind_code(&app, &[storage(code)]).await.expect("checks");
        assert_eq!(checked, found, "{code:?}");
    }
}

#[test]
fn being_behind_names_the_module_and_the_file() {
    assert_eq!(
        Behind::Unapplied {
            schema: "fake",
            version: 3
        }
        .to_string(),
        "fake version 3 is not applied"
    );
    assert_eq!(
        Behind::Changed {
            schema: "fake",
            version: 1
        }
        .to_string(),
        "fake version 1 changed after it was applied"
    );
}

/// A record the module's role cannot read is an error that names the module, not a module behind its code.
#[tokio::test]
async fn a_record_that_cannot_be_read_names_its_module() {
    let database = fresh_database().await.expect("a test database");
    migrate(&database, &[&Fake(&[FIRST])])
        .await
        .expect("migrates");
    connect(&database)
        .await
        .expect("connects")
        .batch_execute("REVOKE SELECT ON fake.migrations FROM tetsu_fake")
        .await
        .expect("revokes");
    let app = pool(&database.as_app()).expect("a pool");
    let error = behind_code(&app, &[storage(&[FIRST])])
        .await
        .expect_err("cannot read");
    let said = error.to_string();
    assert!(
        said.starts_with("could not read fake's applied migrations: ")
            && said.contains("permission denied"),
        "{said}"
    );
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
    let settings = fresh_database_encoded("LATIN1")
        .await
        .expect("a LATIN1 database");

    // Hold the lock elsewhere: a check that ran after taking it would report the lock instead.
    let holder = connect(&settings).await.expect("connects");
    holder
        .query_one("SELECT pg_advisory_lock($1)", &[&MIGRATION_LOCK])
        .await
        .expect("holds the lock");
    let error = migrate_with_wait(
        &settings,
        &[&Fake(&[FIRST])],
        std::time::Duration::from_millis(200),
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("must use UTF8, not LATIN1"),
        "{error}"
    );
    let seeding = require_supported_server(&settings).await.unwrap_err();
    assert_eq!(
        seeding.to_string(),
        "the database must use UTF8, not LATIN1"
    );
    drop(holder);
    assert!(
        !table_exists(&settings, "fake.migrations")
            .await
            .expect("checks")
    );
}

#[tokio::test]
async fn settings_a_migration_makes_do_not_reach_the_next_module() {
    let settings = fresh_database().await.expect("a test database");
    let first = Named(
        "first",
        "first",
        &[Migration {
            version: 1,
            sql: "SET lock_timeout = '5s'; SET search_path = public",
        }],
    );
    let second = Named(
        "second",
        "second",
        &[Migration {
            version: 1,
            sql: "CREATE TABLE second.seen AS SELECT current_setting('lock_timeout') AS lock_timeout",
        }],
    );
    migrate(&settings, &[&first, &second])
        .await
        .expect("migrates");
    let client = connect(&settings).await.expect("connects");
    let row = client
        .query_one("SELECT lock_timeout FROM second.seen", &[])
        .await
        .expect("reads");
    assert_eq!(row.try_get::<_, String>("lock_timeout").expect("maps"), "0");
}

#[tokio::test]
async fn an_unqualified_name_lands_in_the_module_schema() {
    const LOOSE: &[Migration] = &[
        Migration {
            version: 1,
            sql: "CREATE TABLE loose (id int)",
        },
        Migration {
            version: 2,
            sql: "-- tetsu:no-transaction\nCREATE INDEX CONCURRENTLY loose_id ON loose (id)",
        },
    ];
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Named("fake", "fake", LOOSE)])
        .await
        .expect("migrates");
    assert!(table_exists(&settings, "fake.loose").await.expect("checks"));
    assert!(
        table_exists(&settings, "fake.loose_id")
            .await
            .expect("checks")
    );
    assert!(
        !table_exists(&settings, "public.loose")
            .await
            .expect("checks")
    );
}

/// The runner's error for modules it must refuse before touching the database. Fails if it migrates them, or
/// creates any schema first.
async fn refusal(modules: &[&dyn Module]) -> Result<String, tetsu_postgres::Error> {
    let settings = fresh_database().await?;
    let Err(error) = migrate(&settings, modules).await else {
        return Err("the runner accepted them".into());
    };
    let row = connect(&settings)
        .await?
        .query_one(
            "SELECT count(*) AS n FROM pg_namespace WHERE nspname NOT LIKE 'pg\\_%' \
             AND nspname NOT IN ('public', 'information_schema')",
            &[],
        )
        .await?;
    match row.try_get::<_, i64>("n")? {
        0 => Ok(error.to_string()),
        _ => Err(format!("a schema was created before refusing: {error}").into()),
    }
}

#[tokio::test]
async fn refuses_a_schema_that_is_not_a_lowercase_identifier() {
    let long: &'static str = "a".repeat(MAX_SCHEMA + 1).leak();
    for schema in [
        "Billing",
        "",
        "1st",
        "a-b",
        "a b",
        "a;b",
        "pg_x",
        "public",
        long,
        "app",
        "owner",
        "tenancy_lookup",
        "migrate",
        "lookup",
    ] {
        let error = refusal(&[&Named("fake", schema, &[])]).await.expect(schema);
        assert!(error.contains("schema"), "{error}");
    }
}

#[tokio::test]
async fn accepts_any_lowercase_identifier_up_to_the_limit() {
    let settings = fresh_database().await.expect("a test database");
    let longest: &'static str = "a".repeat(MAX_SCHEMA).leak();
    for schema in ["_a", "a1_b", "user", longest] {
        migrate(&settings, &[&Named("fake", schema, &[])])
            .await
            .unwrap_or_else(|error| panic!("{schema}: {error}"));
    }
}

#[tokio::test]
async fn refuses_a_name_or_schema_used_twice() {
    let error = refusal(&[
        &Named("one", "one", &[]),
        &Fake(&[]),
        &Named("one", "two", &[]),
    ])
    .await
    .expect("refused");
    assert!(error.contains("module one is registered twice"), "{error}");
    let error = refusal(&[&Named("one", "same", &[]), &Named("two", "same", &[])])
        .await
        .expect("refused");
    assert!(
        error.contains("schema same belongs to two modules"),
        "{error}"
    );
}

#[tokio::test]
async fn refuses_versions_out_of_order() {
    let error = refusal(&[&Fake(&[SECOND, FIRST])]).await.expect("refused");
    assert!(error.contains("versions must grow"), "{error}");
    let error = refusal(&[&Fake(&[FIRST, CHANGED])]).await.expect("refused");
    assert!(error.contains("versions must grow"), "{error}");
}

#[tokio::test]
async fn refuses_a_new_version_below_one_applied() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, THIRD])])
        .await
        .expect("first run");
    let error = migrate(&settings, &[&Fake(&[FIRST, SECOND, THIRD])])
        .await
        .expect_err("must refuse");
    assert!(
        error
            .to_string()
            .contains("fake version 2 is below 3, which is applied"),
        "{error}"
    );
    assert!(!table_exists(&settings, "fake.b").await.expect("checks"));
}

#[tokio::test]
async fn every_module_is_checked_before_any_file_is_applied() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, THIRD])])
        .await
        .expect("first run");
    let first = Named(
        "first",
        "first",
        &[Migration {
            version: 1,
            sql: "CREATE TABLE first.a (id int)",
        }],
    );
    let error = migrate(&settings, &[&first, &Fake(&[FIRST, SECOND, THIRD])])
        .await
        .expect_err("must refuse");
    assert!(error.to_string().contains("below"), "{error}");
    assert!(!table_exists(&settings, "first.a").await.expect("checks"));
}

#[tokio::test]
async fn a_role_a_migration_takes_reaches_neither_its_record_nor_the_next_module() {
    const SET_ROLE: &[Migration] = &[Migration {
        version: 1,
        sql: "SET ROLE pg_monitor",
    }];
    const SET_AUTHORIZATION: &[Migration] = &[Migration {
        version: 1,
        sql: "SET SESSION AUTHORIZATION pg_monitor",
    }];
    const SEEN: &[Migration] = &[Migration {
        version: 1,
        sql: "CREATE TABLE second.seen AS SELECT current_user::text AS who",
    }];
    for first in [SET_ROLE, SET_AUTHORIZATION] {
        let settings = fresh_database().await.expect("a test database");
        migrate(
            &settings,
            &[
                &Named("first", "first", first),
                &Named("second", "second", SEEN),
            ],
        )
        .await
        .expect("records each file and migrates the next as the session's own user");
        let client = connect(&settings).await.expect("connects");
        let row = client
            .query_one("SELECT who FROM second.seen", &[])
            .await
            .expect("reads");
        assert_eq!(row.try_get::<_, String>("who").expect("maps"), "postgres");
    }
}

#[tokio::test]
async fn refuses_an_applied_version_missing_from_the_code() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, SECOND])])
        .await
        .expect("first run");
    let error = migrate(&settings, &[&Fake(&[FIRST, THIRD])])
        .await
        .expect_err("must refuse");
    assert!(
        error
            .to_string()
            .contains("fake version 2 is applied but missing from the code"),
        "{error}"
    );
    assert!(!table_exists(&settings, "fake.d").await.expect("checks"));
}

#[tokio::test]
async fn a_setting_a_file_makes_does_not_reach_its_own_record() {
    const ENCODED: &[Migration] = &[Migration {
        version: 1,
        sql: "CREATE TABLE enc.a (x text DEFAULT 'é'); SET client_encoding = 'LATIN1'",
    }];
    let settings = fresh_database().await.expect("a test database");
    let module = Named("enc", "enc", ENCODED);
    migrate(&settings, &[&module]).await.expect("first run");
    migrate(&settings, &[&module])
        .await
        .expect("the record holds the file as written, so a rerun finds it unchanged");
}

#[tokio::test]
async fn a_temp_table_from_one_file_does_not_take_over_the_next() {
    const TEMP: &[Migration] = &[Migration {
        version: 1,
        sql: "CREATE TEMP TABLE items (id int)",
    }];
    const ITEMS: &[Migration] = &[Migration {
        version: 1,
        sql: "CREATE TABLE items (id int); INSERT INTO items VALUES (1)",
    }];
    let settings = fresh_database().await.expect("a test database");
    migrate(
        &settings,
        &[&Named("aa", "aa", TEMP), &Named("bb", "bb", ITEMS)],
    )
    .await
    .expect("migrates");
    let client = connect(&settings).await.expect("connects");
    let row = client
        .query_one("SELECT count(*) AS n FROM bb.items", &[])
        .await
        .expect("counts");
    assert_eq!(row.try_get::<_, i64>("n").expect("maps"), 1);
}

#[tokio::test]
async fn a_setting_from_a_file_outside_a_transaction_does_not_reach_the_next() {
    const OUTSIDE: &[Migration] = &[Migration {
        version: 1,
        sql: "-- tetsu:no-transaction\nSET lock_timeout = '7s'",
    }];
    const SEEN: &[Migration] = &[Migration {
        version: 1,
        sql: "CREATE TABLE second.seen AS SELECT current_setting('lock_timeout') AS lock_timeout",
    }];
    let settings = fresh_database().await.expect("a test database");
    migrate(
        &settings,
        &[
            &Named("first", "first", OUTSIDE),
            &Named("second", "second", SEEN),
        ],
    )
    .await
    .expect("migrates");
    let client = connect(&settings).await.expect("connects");
    let row = client
        .query_one("SELECT lock_timeout FROM second.seen", &[])
        .await
        .expect("reads");
    assert_eq!(row.try_get::<_, String>("lock_timeout").expect("maps"), "0");
}

/// The table rights `role` holds on `table`, in Postgres's order.
async fn rights(
    client: &tokio_postgres::Client,
    role: &str,
    table: &str,
) -> Result<Vec<String>, tetsu_postgres::Error> {
    let row = client
        .query_one(
            "SELECT coalesce(array_agg(p ORDER BY n), '{}') AS held
             FROM unnest(ARRAY['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'TRUNCATE', 'REFERENCES', 'TRIGGER',
                               'MAINTAIN']) WITH ORDINALITY AS r(p, n)
             WHERE has_table_privilege($1, $2, p)",
            &[&role, &table],
        )
        .await?;
    Ok(row.try_get("held")?)
}

/// The hosted app role gets rights on every table the migration role creates (#93). It keeps them on a module's
/// own tables but may only read the record of applied files. A later grant, to it, to `PUBLIC`, or passed on from
/// it, is taken back on the next run.
#[tokio::test]
async fn a_role_with_rights_on_every_new_table_may_only_read_the_record() {
    let settings = fresh_database().await.expect("a test database");
    let role = format!("t_app_{}", std::process::id());
    let admin = connect(&settings).await.expect("connects");
    admin
        .batch_execute(&format!(
            "CREATE ROLE {role};
             ALTER DEFAULT PRIVILEGES GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON TABLES TO {role}"
        ))
        .await
        .expect("creates the role");
    // Every result is kept until the role is dropped, so a failure cannot leave it behind in the cluster.
    let checked = async {
        migrate(&settings, &[&Fake(&[FIRST])]).await?;
        let after_create = rights(&admin, &role, "fake.migrations").await?;
        admin
            .batch_execute(&format!(
                "GRANT USAGE ON SCHEMA fake TO {role};
                 GRANT ALL ON fake.migrations TO {role} WITH GRANT OPTION;
                 GRANT TRUNCATE ON fake.migrations TO PUBLIC;
                 SET ROLE {role}; GRANT INSERT ON fake.migrations TO PUBLIC; RESET ROLE"
            ))
            .await?;
        migrate(&settings, &[&Fake(&[FIRST])]).await?;
        let after_rerun = rights(&admin, &role, "fake.migrations").await?;
        let public = rights(&admin, "public", "fake.migrations").await?;
        // The runner only takes rights away, so its own table checked after both runs covers each.
        let own_table = rights(&admin, &role, "fake.a").await?;
        Ok::<_, tetsu_postgres::Error>((after_create, own_table, after_rerun, public))
    }
    .await;
    admin
        .batch_execute(&format!("DROP OWNED BY {role}; DROP ROLE {role}"))
        .await
        .expect("drops the role");
    let (after_create, own_table, after_rerun, public) = checked.expect("migrates and checks");
    assert_eq!(after_create, ["SELECT"]);
    assert_eq!(
        own_table,
        ["SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE"]
    );
    assert_eq!(after_rerun, ["SELECT"]);
    assert!(public.is_empty(), "{public:?}");
}

/// Makes the fake module's roles if no test has yet, since roles belong to the whole server.
async fn make_fake_roles(admin: &tokio_postgres::Client) -> Result<(), tetsu_postgres::Error> {
    for role in ["tetsu_fake", "tetsu_fake_lookup"] {
        admin
            .batch_execute(&format!(
                "DO $$ BEGIN CREATE ROLE {role} NOLOGIN;
                 EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$"
            ))
            .await?;
    }
    Ok(())
}

/// A runner that can create roles, and an app role that gets rights on every table it creates. Other tests share the
/// module's roles across the server and made them first, so the runner is given what making them itself would have
/// given it.
async fn make_runner(
    admin: &tokio_postgres::Client,
    runner: &str,
    app: &str,
    database: &str,
) -> Result<(), tetsu_postgres::Error> {
    make_fake_roles(admin).await?;
    admin
        .batch_execute(&format!(
            "CREATE ROLE {runner} LOGIN CREATEROLE; CREATE ROLE {app};
             GRANT CREATE ON DATABASE {database} TO {runner};
             GRANT tetsu_fake, tetsu_fake_lookup TO {runner} WITH ADMIN OPTION;
             ALTER DEFAULT PRIVILEGES FOR ROLE {runner} GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {app}"
        ))
        .await?;
    Ok(())
}

/// As on Neon: the runner owns what it creates and can create roles but is no superuser, so it needs its own rights
/// on the record, and the app role gets rights on every table it creates. A file that grants on every table in its schema covers the
/// record too, and loses all but reading before the run ends.
#[tokio::test]
async fn a_runner_that_is_no_superuser_keeps_writing_its_record() {
    const GRANT_ALL: Migration = Migration {
        version: 2,
        sql: "GRANT ALL ON ALL TABLES IN SCHEMA fake TO PUBLIC",
    };
    let database = fresh_database().await.expect("a test database");
    let (runner, app) = (
        format!("t_runner_{}", std::process::id()),
        format!("t_runner_app_{}", std::process::id()),
    );
    let admin = connect(&database).await.expect("connects");
    make_runner(&admin, &runner, &app, database.name())
        .await
        .expect("creates the roles");
    // Assumes the test server trusts local roles without a password, as CI's and check.sh's do.
    let runner_url = database
        .url()
        .replacen("postgres@", &format!("{runner}@"), 1);
    let as_runner = tetsu_postgres::Settings::from_values(&runner_url, Some("disable"));
    let checked = async {
        migrate(&as_runner, &[&Fake(&[FIRST, GRANT_ALL])]).await?;
        let record = rights(&admin, &app, "fake.migrations").await?;
        let public = rights(&admin, "public", "fake.migrations").await?;
        migrate(&as_runner, &[&Fake(&[FIRST, GRANT_ALL, THIRD])]).await?;
        Ok::<_, tetsu_postgres::Error>((record, public))
    }
    .await;
    admin
        .batch_execute(&format!(
            "DROP OWNED BY {app}; DROP OWNED BY {runner}; DROP ROLE {app}; DROP ROLE {runner}"
        ))
        .await
        .expect("drops the roles");
    assert!(
        runner_url.contains(&format!("//{runner}@")),
        "TEST_DATABASE_URL must name user postgres with no password, so this test can run as another role"
    );
    let (record, public) = checked.expect("migrates twice and checks");
    assert_eq!(record, ["SELECT"]);
    assert_eq!(public, ["SELECT"]);
}

const GRANT_ALL_TO_PUBLIC: Migration = Migration {
    version: 2,
    sql: "GRANT ALL ON ALL TABLES IN SCHEMA fake TO PUBLIC",
};

/// The rights every role holds on `table` through `PUBLIC`.
async fn public_rights(
    settings: &tetsu_postgres::Settings,
    table: &str,
) -> Result<Vec<String>, tetsu_postgres::Error> {
    rights(&connect(settings).await?, "public", table).await
}

/// The record is locked down in the transaction that creates it, before any file runs.
#[tokio::test]
async fn a_new_record_is_read_only_with_no_files_to_run() {
    let settings = fresh_database().await.expect("a test database");
    connect(&settings)
        .await
        .expect("connects")
        .batch_execute("ALTER DEFAULT PRIVILEGES GRANT ALL ON TABLES TO PUBLIC")
        .await
        .expect("grants on new tables");
    migrate(&settings, &[&Fake(&[])]).await.expect("migrates");
    assert_eq!(
        public_rights(&settings, "fake.migrations")
            .await
            .expect("checks"),
        ["SELECT"]
    );
}

/// A file's grant on the record is taken back before that file commits, so the next file never sees it.
#[tokio::test]
async fn a_file_that_grants_on_the_record_loses_it_before_the_next_file() {
    const SEEN: Migration = Migration {
        version: 3,
        sql: "CREATE TABLE fake.seen AS SELECT has_table_privilege('public', 'fake.migrations', 'INSERT') AS insert",
    };
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, GRANT_ALL_TO_PUBLIC, SEEN])])
        .await
        .expect("migrates");
    let row = connect(&settings)
        .await
        .expect("connects")
        .query_one("SELECT insert FROM fake.seen", &[])
        .await
        .expect("reads");
    assert!(!row.try_get::<_, bool>("insert").expect("maps"));
}

/// A failed file rolls back its grant along with everything else.
#[tokio::test]
async fn a_failed_run_leaves_the_record_read_only() {
    const FAILS: Migration = Migration {
        version: 3,
        sql: "SELECT nope",
    };
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, GRANT_ALL_TO_PUBLIC, FAILS])])
        .await
        .expect_err("must fail");
    assert_eq!(
        public_rights(&settings, "fake.migrations")
            .await
            .expect("checks"),
        ["SELECT"]
    );
}

/// A later module's file that grants on an earlier module's tables does not open its record.
#[tokio::test]
async fn a_later_module_cannot_open_an_earlier_record() {
    const GRANTS_ON_AA: &[Migration] = &[Migration {
        version: 1,
        sql: "GRANT ALL ON ALL TABLES IN SCHEMA aa TO PUBLIC",
    }];
    let settings = fresh_database().await.expect("a test database");
    migrate(
        &settings,
        &[&Named("aa", "aa", &[]), &Named("bb", "bb", GRANTS_ON_AA)],
    )
    .await
    .expect("migrates");
    assert_eq!(
        public_rights(&settings, "aa.migrations")
            .await
            .expect("checks"),
        ["SELECT"]
    );
}

/// A file outside a transaction that grants on the record loses it before its record is written.
#[tokio::test]
async fn a_file_outside_a_transaction_cannot_open_the_record() {
    const GRANTS_OUTSIDE: Migration = Migration {
        version: 2,
        sql: "-- tetsu:no-transaction\nGRANT ALL ON ALL TABLES IN SCHEMA fake TO PUBLIC",
    };
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, GRANTS_OUTSIDE])])
        .await
        .expect("migrates");
    assert_eq!(
        public_rights(&settings, "fake.migrations")
            .await
            .expect("checks"),
        ["SELECT"]
    );
}

/// Write rights on the record's columns go too, which a table-level check does not see.
#[tokio::test]
async fn a_column_grant_on_the_record_is_taken_back() {
    const GRANTS_COLUMNS: Migration = Migration {
        version: 2,
        sql: "GRANT SELECT ON fake.migrations TO PUBLIC; \
              GRANT INSERT (version, sql), UPDATE (sql) ON fake.migrations TO PUBLIC",
    };
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Fake(&[FIRST, GRANTS_COLUMNS])])
        .await
        .expect("migrates");
    let row = connect(&settings)
        .await
        .expect("connects")
        .query_one(
            "SELECT has_any_column_privilege('public', 'fake.migrations', 'INSERT, UPDATE') AS writes",
            &[],
        )
        .await
        .expect("checks");
    assert!(!row.try_get::<_, bool>("writes").expect("maps"));
}
