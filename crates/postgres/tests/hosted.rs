//! What the hosted chores run against a migrated database: the isolation check after every migrate, and dropping
//! modules' schemas to start them over (#135).

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{check_isolation, connect, drop_module_schemas, migrate, module_schemas};

#[cfg(test)]
mod support {
    use super::*;

    /// A module whose one migration is `sql`, in its own schema.
    pub struct Named(pub &'static str, pub &'static str);

    impl Module for Named {
        fn name(&self) -> &'static str {
            self.0
        }
        fn storage(&self) -> Option<Storage> {
            Some(Storage {
                schema: self.0,
                migrations: Box::leak(Box::new([Migration {
                    version: 1,
                    sql: self.1,
                }])),
            })
        }
    }

    pub const PEOPLE: &str = include_str!("people.sql");
    pub const NOTES: &str = "CREATE TABLE notes (id bigint PRIMARY KEY); COMMENT ON TABLE notes IS 'tetsu:global: test';";

    /// A database migrated with the people module and a second, plain one.
    pub async fn migrated() -> TestDatabase {
        let database = fresh_database().await.expect("a test database");
        migrate(
            &database,
            &[&Named("people", PEOPLE), &Named("journal", NOTES)],
        )
        .await
        .expect("migrates");
        database
    }

    pub async fn schemas(database: &TestDatabase) -> Vec<String> {
        let client = connect(database).await.expect("connects");
        module_schemas(&client).await.expect("lists")
    }
}

use support::{Named, PEOPLE, migrated, schemas};

#[tokio::test]
async fn a_database_that_keeps_the_rules_has_no_isolation_problem() {
    let database = migrated().await;
    assert_eq!(
        check_isolation(&database).await.expect("checks"),
        Vec::<String>::new()
    );
}

/// The check sees what the catalog shows, as hosted test 8 did.
#[tokio::test]
async fn the_isolation_check_reports_a_broken_rule() {
    let database = migrated().await;
    connect(&database)
        .await
        .expect("connects")
        .batch_execute("ALTER TABLE people.people NO FORCE ROW LEVEL SECURITY")
        .await
        .expect("breaks a rule");
    let problems = check_isolation(&database).await.expect("checks");
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("people.people")),
        "{problems:?}"
    );
}

/// Dropping takes a module's record of applied files with it, so the next migrate builds the schema afresh.
#[tokio::test]
async fn dropped_schemas_are_built_afresh_by_the_next_migrate() {
    let database = migrated().await;
    drop_module_schemas(&database, &["people".into(), "journal".into()])
        .await
        .expect("drops");
    assert_eq!(schemas(&database).await, Vec::<String>::new());
    migrate(&database, &[&Named("people", PEOPLE)])
        .await
        .expect("migrates afresh");
    assert_eq!(schemas(&database).await, ["people"]);
}

/// A name that is not a module's schema here drops nothing at all, whatever it holds.
#[tokio::test]
async fn only_modules_schemas_are_dropped_and_all_or_none() {
    let database = migrated().await;
    for names in [
        vec!["people".to_string(), "public".to_string()],
        vec!["pg_catalog".to_string()],
        vec!["people\"; DROP SCHEMA journal; --".to_string()],
        vec!["nobody".to_string()],
    ] {
        let refused = drop_module_schemas(&database, &names)
            .await
            .expect_err("refused");
        assert!(
            refused
                .to_string()
                .ends_with("is not a module's schema here"),
            "{refused}"
        );
    }
    assert_eq!(schemas(&database).await, ["journal", "people"]);
}
