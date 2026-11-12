//! Tetsu's own roles, made by the migration runner (ADR-0042).

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::fresh_database;
use tetsu_postgres::{APP_ROLE, Settings, connect, lookup_role, migrate, module_role};

/// A module with a chosen schema. Roles belong to the whole server, so tests that make roles their own way use a
/// schema no other test uses.
struct Named(&'static str, &'static [Migration]);

impl Module for Named {
    fn name(&self) -> &'static str {
        self.0
    }
    fn storage(&self) -> Option<Storage> {
        Some(Storage {
            schema: self.0,
            migrations: self.1,
        })
    }
}

const TABLE: Migration = Migration {
    version: 1,
    sql: "CREATE TABLE things (id int)",
};

fn unique(prefix: &str) -> &'static str {
    format!("{prefix}_{}", std::process::id()).leak()
}

/// Whether `member` belongs to `role`, and with which options.
async fn membership(
    client: &tokio_postgres::Client,
    member: &str,
    role: &str,
) -> Result<Option<(bool, bool)>, tetsu_postgres::Error> {
    let row = client
        .query_opt(
            "SELECT m.inherit_option, m.set_option FROM pg_auth_members m
             JOIN pg_roles r ON r.oid = m.roleid JOIN pg_roles u ON u.oid = m.member
             WHERE r.rolname = $1 AND u.rolname = $2",
            &[&role, &member],
        )
        .await?;
    Ok(row
        .map(|row| Ok::<_, tokio_postgres::Error>((row.try_get(0)?, row.try_get(1)?)))
        .transpose()?)
}

async fn has(
    client: &tokio_postgres::Client,
    role: &str,
    table: &str,
    privilege: &str,
) -> Result<bool, tetsu_postgres::Error> {
    let row = client
        .query_one(
            "SELECT has_table_privilege($1, $2, $3) AS held",
            &[&role, &table, &privilege],
        )
        .await?;
    Ok(row.try_get("held")?)
}

#[tokio::test]
async fn the_app_role_acts_as_a_module_role_only_by_switching_to_it() {
    let database = fresh_database().await.expect("a test database");
    migrate(&database, &[&Named("roles_shape", &[TABLE])])
        .await
        .expect("migrates");
    let client = connect(&database).await.expect("connects");
    let (module, lookup) = (module_role("roles_shape"), lookup_role("roles_shape"));
    assert_eq!(
        membership(&client, APP_ROLE, &module).await.expect("reads"),
        Some((false, true)),
        "granted WITH INHERIT FALSE, SET TRUE"
    );
    assert_eq!(
        membership(&client, APP_ROLE, &lookup).await.expect("reads"),
        None,
        "the app role never acts as a lookup role"
    );
    for privilege in ["SELECT", "INSERT", "UPDATE", "DELETE"] {
        assert!(
            has(&client, &module, "roles_shape.things", privilege)
                .await
                .expect("reads"),
            "{privilege}"
        );
        assert!(
            !has(&client, APP_ROLE, "roles_shape.things", privilege)
                .await
                .expect("reads"),
            "the app role holds no {privilege} of its own"
        );
    }
    assert!(
        !has(&client, &module, "roles_shape.things", "TRUNCATE")
            .await
            .expect("reads")
    );
}

/// As on Neon: the runner is no superuser, only able to create roles, and owns what it creates.
#[tokio::test]
async fn a_runner_that_only_creates_roles_can_migrate() {
    let database = fresh_database().await.expect("a test database");
    let (runner, schema) = (unique("t_owner"), unique("owned"));
    let admin = connect(&database).await.expect("connects");
    admin
        .batch_execute(&format!(
            "CREATE ROLE {runner} LOGIN CREATEROLE; GRANT CREATE ON DATABASE {} TO {runner}",
            database.name()
        ))
        .await
        .expect("creates the runner");
    let url = database
        .url()
        .replacen("postgres@", &format!("{runner}@"), 1);
    let as_runner = Settings::from_values(&url, Some("disable"));
    let migrated = migrate(&as_runner, &[&Named(schema, &[TABLE])]).await;
    let roles = [module_role(schema), lookup_role(schema)];
    let app_member = membership(&admin, APP_ROLE, &roles[0]).await;
    // One role at a time: dropping several at once trips over the runner's default privileges.
    for role in [&roles[0], &roles[1], &runner.to_string()] {
        admin
            .batch_execute(&format!("DROP OWNED BY {role}"))
            .await
            .expect("drops what the role holds");
    }
    admin
        .batch_execute(&format!("DROP ROLE {}, {}, {runner}", roles[0], roles[1]))
        .await
        .expect("drops the roles");
    migrated.expect("migrates as a role that only creates roles");
    assert_eq!(app_member.expect("reads"), Some((false, true)));
}

/// Roles belong to the whole server, so databases migrating the same new module at once race to make them.
#[tokio::test]
async fn databases_migrating_one_new_module_at_once_all_finish() {
    let schema = unique("raced");
    let mut databases = Vec::new();
    for _ in 0..4 {
        databases.push(fresh_database().await.expect("a test database"));
    }
    // One task, so the module needs no `Send`; the runs still overlap on their own connections.
    let module = Named(schema, &[TABLE]);
    let modules: [&dyn Module; 1] = [&module];
    let results = tokio::join!(
        migrate(&databases[0], &modules),
        migrate(&databases[1], &modules),
        migrate(&databases[2], &modules),
        migrate(&databases[3], &modules),
    );
    let failures: Vec<String> = [results.0, results.1, results.2, results.3]
        .into_iter()
        .filter_map(|result| result.err().map(|error| error.to_string()))
        .collect();
    let admin = connect(&databases[0]).await.expect("connects");
    for database in &databases {
        let client = connect(database).await.expect("connects");
        client
            .batch_execute(&format!(
                "DROP OWNED BY {}, {}",
                module_role(schema),
                lookup_role(schema)
            ))
            .await
            .expect("drops what the roles hold here");
    }
    admin
        .batch_execute(&format!(
            "DROP ROLE {}, {}",
            module_role(schema),
            lookup_role(schema)
        ))
        .await
        .expect("drops the roles");
    assert!(failures.is_empty(), "{failures:?}");
}

/// As Neon's own roles do: a runner that skips row-level security without being a superuser is refused.
#[tokio::test]
async fn a_runner_that_skips_row_level_security_is_refused() {
    let database = fresh_database().await.expect("a test database");
    let runner = unique("t_bypass");
    let admin = connect(&database).await.expect("connects");
    admin
        .batch_execute(&format!(
            "CREATE ROLE {runner} LOGIN CREATEROLE BYPASSRLS; GRANT CREATE ON DATABASE {} TO {runner}",
            database.name()
        ))
        .await
        .expect("creates the runner");
    let url = database
        .url()
        .replacen("postgres@", &format!("{runner}@"), 1);
    let refused = migrate(
        &Settings::from_values(&url, Some("disable")),
        &[&Named(unique("bypassed"), &[TABLE])],
    )
    .await;
    admin
        .batch_execute(&format!("DROP OWNED BY {runner}; DROP ROLE {runner}"))
        .await
        .expect("drops the runner");
    let error = refused.expect_err("refuses").to_string();
    assert!(
        error.contains(&format!("role {runner} skips row-level security")),
        "{error}"
    );
}
