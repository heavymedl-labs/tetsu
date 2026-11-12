//! The catalog checks: a table that follows ADR-0042 passes, and each way of breaking it is caught.

use tetsu_kernel::{Migration, Module, Storage};
use tetsu_postgres::test_support::fresh_database;
use tetsu_postgres::{connect, isolation_violations, migrate};

#[cfg(test)]
mod support {
    use super::*;

    pub struct Checked(pub &'static str, pub &'static [Migration]);

    impl Module for Checked {
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

    /// A tenant-owned table as ADR-0042 shapes one, in schema `checked`.
    pub fn tenant_owned(name: &str) -> String {
        format!(
            "CREATE TABLE {name} (tenant_id bigint NOT NULL, id uuid NOT NULL DEFAULT uuidv7(), PRIMARY KEY (tenant_id, id));
             COMMENT ON TABLE {name} IS 'tetsu:tenant-owned';
             ALTER TABLE {name} ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;
             CREATE POLICY tenant ON {name} TO tetsu_checked
               USING (tenant_id = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint));
             CREATE POLICY tenant_only ON {name} AS RESTRICTIVE TO tetsu_checked
               USING (tenant_id = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint));"
        )
    }

    /// The problems the catalog shows after migrating `sql` into schema `checked`, and `other` into schema
    /// `checked_other`.
    pub async fn problems(sql: String, other: &'static str) -> Vec<String> {
        let database = fresh_database().await.expect("a test database");
        let first: &'static [Migration] = vec![Migration {
            version: 1,
            sql: sql.leak(),
        }]
        .leak();
        let second: &'static [Migration] = vec![Migration {
            version: 1,
            sql: other,
        }]
        .leak();
        migrate(
            &database,
            &[
                &Checked("checked_other", second),
                &Checked("checked", first),
            ],
        )
        .await
        .expect("migrates");
        let client = connect(&database).await.expect("connects");
        isolation_violations(&client, &["checked", "checked_other"])
            .await
            .expect("checks")
    }

    pub async fn caught(sql: String, expected: &str) {
        let found = problems(sql, "").await;
        assert!(
            found.iter().any(|problem| problem.contains(expected)),
            "expected {expected:?} among {found:?}"
        );
    }
}

use support::{caught, problems, tenant_owned};

#[tokio::test]
async fn a_table_that_follows_the_rules_passes() {
    let global = "CREATE TABLE registry (id bigint PRIMARY KEY);
                  COMMENT ON TABLE registry IS 'tetsu:global: read before a tenant is known'";
    assert_eq!(
        problems(format!("{}\n{global}", tenant_owned("good")), "").await,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn an_undeclared_table_is_caught() {
    caught(
        "CREATE TABLE loose (id int)".into(),
        "checked.loose is declared neither tenant-owned nor global",
    )
    .await;
    caught(
        "CREATE TABLE vague (id int); COMMENT ON TABLE vague IS 'tetsu:global:'".into(),
        "checked.vague is declared neither",
    )
    .await;
}

#[tokio::test]
async fn a_tenant_owned_table_without_forced_rls_or_both_policies_is_caught() {
    caught(
        format!(
            "{} ALTER TABLE good NO FORCE ROW LEVEL SECURITY;",
            tenant_owned("good")
        ),
        "checked.good lacks row-level security, enabled and forced",
    )
    .await;
    caught(
        format!("{} DROP POLICY tenant_only ON good;", tenant_owned("good")),
        "checked.good lacks a restrictive tenant policy for tetsu_checked",
    )
    .await;
    caught(
        format!("{} DROP POLICY tenant ON good;", tenant_owned("good")),
        "checked.good lacks a permissive tenant policy for tetsu_checked",
    )
    .await;
    caught(
        format!(
            "{} CREATE POLICY everyone ON good USING (true);",
            tenant_owned("good")
        ),
        "checked.good has policy everyone for every role",
    )
    .await;
}

#[tokio::test]
async fn keys_that_do_not_lead_with_the_tenant_are_caught() {
    caught(
        "CREATE TABLE bare (id int PRIMARY KEY); COMMENT ON TABLE bare IS 'tetsu:tenant-owned'"
            .into(),
        "checked.bare has no tenant_id column",
    )
    .await;
    caught(
        format!(
            "{} ALTER TABLE good DROP CONSTRAINT good_pkey, ADD PRIMARY KEY (id, tenant_id);",
            tenant_owned("good")
        ),
        "checked.good's primary key does not start with tenant_id",
    )
    .await;
    caught(
        format!(
            "{} ALTER TABLE good ADD COLUMN email text, ADD UNIQUE (email);",
            tenant_owned("good")
        ),
        "does not start with tenant_id",
    )
    .await;
    caught(
        format!(
            "CREATE TABLE registry (id uuid PRIMARY KEY); COMMENT ON TABLE registry IS 'tetsu:global: test';
             {}
             ALTER TABLE child ADD COLUMN parent uuid,
               ADD CONSTRAINT to_parent FOREIGN KEY (parent) REFERENCES registry (id);",
            tenant_owned("child")
        ),
        "checked.child's foreign key to_parent leaves out tenant_id",
    )
    .await;
}

#[tokio::test]
async fn a_table_owned_by_another_role_is_caught() {
    caught(
        format!(
            "{} ALTER TABLE good OWNER TO tetsu_checked;",
            tenant_owned("good")
        ),
        "checked.good is owned by tetsu_checked, not its schema's owner",
    )
    .await;
}

#[tokio::test]
async fn rights_the_app_role_holds_of_its_own_are_caught() {
    caught(
        format!(
            "{} GRANT SELECT ON good TO tetsu_app;",
            tenant_owned("good")
        ),
        "tetsu_app holds SELECT on checked.good of its own",
    )
    .await;
    caught(
        "GRANT USAGE ON SCHEMA checked TO tetsu_app".into(),
        "tetsu_app holds USAGE on schema checked of its own",
    )
    .await;
    caught(
        "ALTER DEFAULT PRIVILEGES IN SCHEMA checked GRANT SELECT ON TABLES TO tetsu_app".into(),
        "tetsu_app is named in default privileges of",
    )
    .await;
}

#[tokio::test]
async fn rights_a_module_role_must_not_hold_are_caught() {
    caught(
        format!(
            "{} GRANT TRUNCATE ON good TO tetsu_checked;",
            tenant_owned("good")
        ),
        "tetsu_checked holds TRUNCATE on checked.good",
    )
    .await;
    let found = problems(
        "GRANT SELECT ON checked_other.theirs TO tetsu_checked".into(),
        "CREATE TABLE theirs (id int); COMMENT ON TABLE theirs IS 'tetsu:global: test'",
    )
    .await;
    assert!(
        found.contains(
            &"tetsu_checked holds SELECT on checked_other.theirs, in another module's schema"
                .to_string()
        ),
        "{found:?}"
    );
}

/// Default privileges reach partitions too, so a module's partitioned table must take them back.
#[tokio::test]
async fn rights_on_a_partition_are_caught() {
    caught(
        "CREATE TABLE parts (tenant_id bigint NOT NULL, id uuid NOT NULL, PRIMARY KEY (tenant_id, id))
           PARTITION BY HASH (tenant_id);
         COMMENT ON TABLE parts IS 'tetsu:global: test';
         CREATE TABLE parts_0 PARTITION OF parts FOR VALUES WITH (MODULUS 1, REMAINDER 0);
         COMMENT ON TABLE parts_0 IS 'tetsu:global: test'"
            .into(),
        "tetsu_checked holds SELECT on partition checked.parts_0",
    )
    .await;
}

#[tokio::test]
async fn a_lookup_function_must_be_narrow() {
    let found = problems(
        // The runner's defaults keep a new function from `PUBLIC`, so the grant is spelled out here.
        "CREATE FUNCTION wide() RETURNS int LANGUAGE sql SECURITY DEFINER AS 'SELECT 1';
         GRANT EXECUTE ON FUNCTION wide() TO PUBLIC"
            .into(),
        "",
    )
    .await;
    for why in [
        "is not owned by tetsu_checked_lookup",
        "sets no search_path",
        "anyone may run it",
    ] {
        assert!(
            found.iter().any(|problem| problem.contains(why)),
            "{why} among {found:?}"
        );
    }
    let narrow = problems(
        "CREATE FUNCTION narrow() RETURNS int LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog, pg_temp
           AS 'SELECT 1';
         REVOKE EXECUTE ON FUNCTION narrow() FROM PUBLIC;
         GRANT EXECUTE ON FUNCTION narrow() TO tetsu_checked;
         ALTER FUNCTION narrow() OWNER TO tetsu_checked_lookup"
            .into(),
        "",
    )
    .await;
    assert_eq!(narrow, Vec::<String>::new());
}

#[tokio::test]
async fn a_tenant_policy_must_say_the_tenant_rule() {
    caught(
        tenant_owned("good").replace(
            "USING (tenant_id = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint));",
            "USING (true);",
        ),
        "checked.good lacks a permissive tenant policy for tetsu_checked",
    )
    .await;
    caught(
        tenant_owned("good").replace(
            "CREATE POLICY tenant ON good TO",
            "CREATE POLICY tenant ON good FOR SELECT TO",
        ),
        "checked.good lacks a permissive tenant policy for tetsu_checked",
    )
    .await;
}

#[tokio::test]
async fn the_app_role_s_rights_on_sequences_and_records_are_caught() {
    let found = problems(
        "CREATE TABLE counted (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
         COMMENT ON TABLE counted IS 'tetsu:global: test';
         GRANT USAGE ON SEQUENCE counted_id_seq TO tetsu_app;
         GRANT SELECT ON migrations TO tetsu_app"
            .into(),
        "",
    )
    .await;
    for expected in [
        "tetsu_app holds USAGE on checked.counted_id_seq of its own",
        "tetsu_app holds SELECT on checked.migrations of its own",
    ] {
        assert!(
            found.contains(&expected.to_string()),
            "{expected} among {found:?}"
        );
    }
}

/// Roles belong to the whole server, so these cases use an application role of their own and a module no other
/// test migrates, never the shared ones other tests check at the same moment.
#[tokio::test]
async fn rights_reached_through_other_roles_or_ownership_are_caught() {
    let database = fresh_database().await.expect("a test database");
    let id = std::process::id();
    let (app, chained) = (format!("t_iso_app_{id}"), format!("t_iso_chained_{id}"));
    let schema: &'static str = format!("iso_{id}").leak();
    let client = connect(&database).await.expect("connects");
    client
        .batch_execute(&format!(
            "CREATE ROLE {app}; CREATE ROLE {chained} BYPASSRLS;
             GRANT pg_read_all_data TO {app}"
        ))
        .await
        .expect("creates the roles");
    let checked = async {
        migrate(&database, &[&support::Checked(schema, &[])]).await?;
        client
            .batch_execute(&format!(
                "GRANT {chained} TO tetsu_{schema} WITH SET TRUE;
                 GRANT tetsu_{schema} TO {app} WITH INHERIT FALSE, SET TRUE;
                 CREATE TABLE {schema}.mine (id int); ALTER TABLE {schema}.mine OWNER TO {app}"
            ))
            .await?;
        tetsu_postgres::isolation_violations_for(&client, &[schema], &app).await
    }
    .await;
    client
        .batch_execute(&format!(
            "DROP OWNED BY {app}; DROP OWNED BY tetsu_{schema}, tetsu_{schema}_lookup;
             DROP ROLE {app}, {chained}, tetsu_{schema}, tetsu_{schema}_lookup"
        ))
        .await
        .expect("drops the roles");
    let found = checked.expect("checks");
    for expected in [
        format!("{app} inherits the rights of pg_read_all_data"),
        format!("{chained} skips row-level security"),
        format!("{app} owns {schema}.mine"),
    ] {
        assert!(found.contains(&expected), "{expected} among {found:?}");
    }
}
