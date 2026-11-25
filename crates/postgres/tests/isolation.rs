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
        let mut client = connect(&database).await.expect("connects");
        isolation_violations(&mut client, &["checked", "checked_other"])
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
        "checked.good's primary key is not (tenant_id, id) with a uuid id",
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
            // A key to a tenant-owned table checks the referenced row whatever its tenant.
            "{} ALTER TABLE registry ADD CONSTRAINT one_id UNIQUE (id);
             {}
             ALTER TABLE child ADD COLUMN parent uuid,
               ADD CONSTRAINT to_parent FOREIGN KEY (parent) REFERENCES registry (id);",
            tenant_owned("registry"),
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

/// A module's lookup role may read its own tenant-owned tables, as ADR-0042 lets its one-input functions do, such as
/// finding an invite by its link before the business is known (#111), through a policy that says why.
#[tokio::test]
async fn a_lookup_role_may_read_its_modules_tenant_owned_tables() {
    let read = format!(
        "{} CREATE POLICY lookup ON good FOR SELECT TO tetsu_checked_lookup USING (true);
         COMMENT ON POLICY lookup ON good IS 'tetsu:lookup: invites, by their link';",
        tenant_owned("good")
    );
    assert_eq!(problems(read, "").await, Vec::<String>::new());
}

/// A lookup policy that does not say why, or says nothing after the marker, is caught, so every way across tenants
/// is written down for review.
#[tokio::test]
async fn a_lookup_policy_must_say_why() {
    let not_the_rule = "has policy lookup that is not the tenant rule for tetsu_checked alone";
    for comment in [
        "",
        "COMMENT ON POLICY lookup ON good IS 'tetsu:lookup: ';",
        "COMMENT ON POLICY lookup ON good IS 'invites';",
    ] {
        caught(
            format!(
                "{} CREATE POLICY lookup ON good FOR SELECT TO tetsu_checked_lookup USING (true); {comment}",
                tenant_owned("good")
            ),
            not_the_rule,
        )
        .await;
    }
}

/// A table inheriting another module's tenant-owned table is caught, since a query of the parent reads the child's
/// rows under the parent's policies alone.
#[tokio::test]
async fn a_child_of_another_schemas_table_is_caught() {
    let parent: &'static str = tenant_owned("parent")
        .replace("tetsu_checked", "tetsu_checked_other")
        .leak();
    let child = "CREATE TABLE theirs () INHERITS (checked_other.parent);
         COMMENT ON TABLE theirs IS 'tetsu:tenant-owned';"
        .to_string();
    let found = problems(child, parent).await;
    let expected = "checked.theirs inherits from checked_other.parent, a table of another schema";
    assert!(
        found.iter().any(|problem| problem == expected),
        "{expected} among {found:?}"
    );
}

/// A lookup role's policy reads and nothing more: one that writes, checks rows, restricts, or names another module's
/// lookup role is caught.
#[tokio::test]
async fn a_lookup_policy_that_does_more_than_read_is_caught() {
    let not_the_rule = "has policy lookup that is not the tenant rule for tetsu_checked alone";
    for policy in [
        "CREATE POLICY lookup ON good TO tetsu_checked_lookup USING (true);",
        "CREATE POLICY lookup ON good FOR DELETE TO tetsu_checked_lookup USING (true);",
        "CREATE POLICY lookup ON good FOR INSERT TO tetsu_checked_lookup WITH CHECK (true);",
        "CREATE POLICY lookup ON good AS RESTRICTIVE FOR SELECT TO tetsu_checked_lookup USING (true);",
        "CREATE POLICY lookup ON good FOR SELECT TO tetsu_checked_other_lookup USING (true);",
        "CREATE POLICY lookup ON good FOR SELECT TO tetsu_checked_lookup, tetsu_checked USING (true);",
    ] {
        let said = "COMMENT ON POLICY lookup ON good IS 'tetsu:lookup: test';";
        caught(
            format!("{} {policy} {said}", tenant_owned("good")),
            not_the_rule,
        )
        .await;
    }
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
    let mut client = connect(&database).await.expect("connects");
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
                // The runner grants every module role to the shared application role, which other tests check, so
                // this module's role leaves it before reaching the bad role.
                "REVOKE tetsu_{schema} FROM tetsu_app;
                 GRANT {chained} TO tetsu_{schema} WITH SET TRUE;
                 GRANT tetsu_{schema} TO {app} WITH INHERIT FALSE, SET TRUE;
                 CREATE TABLE {schema}.mine (id int); ALTER TABLE {schema}.mine OWNER TO {app}"
            ))
            .await?;
        tetsu_postgres::isolation_violations_for(&mut client, &[schema], &app).await
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

#[tokio::test]
async fn rights_through_public_or_on_single_columns_are_caught() {
    for (sql, expected) in [
        (
            format!("{} GRANT SELECT ON good TO PUBLIC;", tenant_owned("good")),
            "PUBLIC holds SELECT on checked.good of its own",
        ),
        (
            "GRANT USAGE ON SCHEMA checked TO PUBLIC".to_string(),
            "PUBLIC holds USAGE on schema checked of its own",
        ),
        (
            format!(
                "{} GRANT SELECT (id) ON good TO tetsu_app;",
                tenant_owned("good")
            ),
            "tetsu_app holds SELECT on checked.good (column id) of its own",
        ),
    ] {
        caught(sql, expected).await;
    }
}

#[tokio::test]
async fn views_must_run_as_their_caller_and_materialized_views_are_refused() {
    caught(
        format!(
            "{} CREATE VIEW seen AS SELECT * FROM good;",
            tenant_owned("good")
        ),
        "checked.seen is a view that runs with its owner's rights",
    )
    .await;
    caught(
        "CREATE MATERIALIZED VIEW kept AS SELECT 1 AS one".into(),
        "checked.kept is a materialized view, which row-level security does not reach",
    )
    .await;
    let invoker = format!(
        "{} CREATE VIEW seen WITH (security_invoker = true) AS SELECT * FROM good;",
        tenant_owned("good")
    );
    assert_eq!(problems(invoker, "").await, Vec::<String>::new());
}

#[tokio::test]
async fn policies_for_the_owner_and_wide_exclusions_are_caught() {
    caught(
        format!(
            "{} CREATE POLICY own ON good TO postgres USING (true);",
            tenant_owned("good")
        ),
        "checked.good has policy own for its owner, postgres",
    )
    .await;
    caught(
        format!(
            "{} ALTER TABLE good ADD CONSTRAINT one_id EXCLUDE USING btree (id WITH =);",
            tenant_owned("good")
        ),
        "checked.good's exclusion constraint one_id does not start with tenant_id",
    )
    .await;
}

#[tokio::test]
async fn a_lookup_must_search_only_the_catalog_then_its_own_temp_schema() {
    caught(
        "CREATE FUNCTION loose() RETURNS int LANGUAGE sql SECURITY DEFINER SET search_path = pg_temp, pg_catalog
           AS 'SELECT 1';
         REVOKE EXECUTE ON FUNCTION loose() FROM PUBLIC;
         ALTER FUNCTION loose() OWNER TO tetsu_checked_lookup"
            .into(),
        "function checked.loose() runs as its owner but sets no search_path of pg_catalog, pg_temp",
    )
    .await;
}

#[tokio::test]
async fn database_settings_that_pick_a_tenant_or_role_are_caught() {
    caught(
        "DO $$ BEGIN
           EXECUTE format('ALTER ROLE tetsu_app IN DATABASE %I SET app.tenant_id = %L', current_database(), '1');
         END $$"
            .into(),
        "database settings for tetsu_app set app.tenant_id",
    )
    .await;
}

#[tokio::test]
async fn a_module_role_reaching_another_module_s_schema_or_functions_is_caught() {
    let found = problems(
        "GRANT USAGE ON SCHEMA checked_other TO tetsu_checked;
         GRANT EXECUTE ON FUNCTION checked_other.theirs() TO tetsu_checked"
            .into(),
        "CREATE FUNCTION theirs() RETURNS int LANGUAGE sql AS 'SELECT 1'",
    )
    .await;
    for expected in [
        "tetsu_checked holds USAGE on schema checked_other, another module's",
        "tetsu_checked holds EXECUTE on function checked_other.theirs(), in another module's schema",
    ] {
        assert!(
            found.contains(&expected.to_string()),
            "{expected} among {found:?}"
        );
    }
}

/// Built-in roles and attributes, checked against an application role of its own, never the shared one.
#[tokio::test]
async fn built_in_roles_grants_and_attributes_the_app_role_holds_are_caught() {
    let database = fresh_database().await.expect("a test database");
    let id = std::process::id();
    let (app, owner) = (format!("t_iso2_app_{id}"), format!("t_iso2_owner_{id}"));
    let mut client = connect(&database).await.expect("connects");
    let checked = async {
        client
            .batch_execute(&format!(
                "CREATE ROLE {app} CREATEROLE; CREATE ROLE {owner} BYPASSRLS;
                 GRANT pg_read_all_data TO {app} WITH INHERIT FALSE, SET TRUE;
                 GRANT pg_write_all_data TO {app} WITH INHERIT FALSE, SET FALSE, ADMIN TRUE;
                 CREATE SCHEMA owned AUTHORIZATION {owner};
                 CREATE TABLE owned.migrations (version int); ALTER TABLE owned.migrations OWNER TO {owner}"
            ))
            .await?;
        tetsu_postgres::isolation_violations_for(&mut client, &["owned"], &app).await
    }
    .await;
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS owned CASCADE; DROP OWNED BY {owner}; DROP ROLE IF EXISTS {app}, {owner}"
        ))
        .await
        .expect("drops the roles");
    let found = checked.expect("checks");
    for expected in [
        format!("{app} can act as pg_read_all_data, which is no module's role"),
        format!("{app} may grant pg_write_all_data"),
        format!("{app} can create roles or databases, or replicate"),
        format!("the owner {owner} skips row-level security"),
    ] {
        assert!(found.contains(&expected), "{expected} among {found:?}");
    }
}

#[tokio::test]
async fn rights_of_lookup_roles_and_the_app_role_on_lookups_are_caught() {
    let found = problems(
        "CREATE FUNCTION look() RETURNS int LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog, pg_temp
           AS 'SELECT 1';
         REVOKE EXECUTE ON FUNCTION look() FROM PUBLIC;
         GRANT EXECUTE ON FUNCTION look() TO tetsu_app;
         ALTER FUNCTION look() OWNER TO tetsu_checked_lookup;
         GRANT SELECT ON checked_other.theirs TO tetsu_checked_lookup;
         GRANT CREATE ON SCHEMA checked TO tetsu_checked_lookup"
            .into(),
        "CREATE TABLE theirs (id int); COMMENT ON TABLE theirs IS 'tetsu:global: test'",
    )
    .await;
    for expected in [
        "tetsu_app holds EXECUTE on function checked.look() of its own",
        "tetsu_checked_lookup holds SELECT on checked_other.theirs, in another module's schema",
        "tetsu_checked_lookup holds CREATE on schema checked",
    ] {
        assert!(
            found.contains(&expected.to_string()),
            "{expected} among {found:?}"
        );
    }
}

#[tokio::test]
async fn an_exclusion_must_compare_tenants_for_equality() {
    caught(
        format!(
            "CREATE EXTENSION btree_gist SCHEMA public;
             {} ALTER TABLE good ADD COLUMN room int,
               ADD CONSTRAINT one_room EXCLUDE USING gist (tenant_id WITH <>, room WITH =);",
            tenant_owned("good")
        ),
        "checked.good's exclusion constraint one_room does not start with tenant_id",
    )
    .await;
}

#[tokio::test]
async fn objects_outside_the_module_schemas_and_rules_are_caught() {
    caught(
        "CREATE FUNCTION public.peek() RETURNS int LANGUAGE sql AS 'SELECT 1'".into(),
        "function public.peek() lies outside every module's schema",
    )
    .await;
    caught(
        format!(
            "{} CREATE RULE echo AS ON INSERT TO good DO ALSO NOTIFY good;",
            tenant_owned("good")
        ),
        "checked.good has rule echo, which runs with its owner's rights",
    )
    .await;
}

#[tokio::test]
async fn settings_and_view_options_are_read_whatever_their_spelling() {
    caught(
        "DO $$ BEGIN
           EXECUTE format('ALTER ROLE tetsu_app IN DATABASE %I SET \"App.Tenant_Id\" = %L', current_database(), '1');
         END $$"
            .into(),
        "database settings for tetsu_app set App.Tenant_Id",
    )
    .await;
    let invoker = format!(
        "{} CREATE VIEW seen WITH (security_invoker = on) AS SELECT * FROM good;",
        tenant_owned("good")
    );
    assert_eq!(problems(invoker, "").await, Vec::<String>::new());
}

/// Module roles belong to nothing, and a policy cannot reach the owner through a group. Run with roles and a
/// module of its own, so no shared role is touched.
#[tokio::test]
async fn roles_a_module_role_belongs_to_and_policies_for_the_owner_s_groups_are_caught() {
    let database = fresh_database().await.expect("a test database");
    let id = std::process::id();
    let (app, owner, hidden, group) = (
        format!("t_iso3_app_{id}"),
        format!("t_iso3_owner_{id}"),
        format!("t_iso3_hidden_{id}"),
        format!("t_iso3_group_{id}"),
    );
    let schema = format!("iso3_{id}");
    let module = format!("tetsu_{schema}");
    let mut client = connect(&database).await.expect("connects");
    let checked = async {
        client
            .batch_execute(&format!(
                "CREATE ROLE {app}; CREATE ROLE {owner}; CREATE ROLE {hidden}; CREATE ROLE {group};
                 CREATE ROLE {module} NOLOGIN;
                 GRANT {module} TO {app} WITH INHERIT FALSE, SET TRUE;
                 GRANT {hidden} TO {module} WITH INHERIT TRUE, SET FALSE;
                 GRANT {group} TO {owner};
                 CREATE SCHEMA {schema} AUTHORIZATION {owner};
                 CREATE TABLE {schema}.good (tenant_id bigint NOT NULL, id uuid NOT NULL, PRIMARY KEY (tenant_id, id));
                 ALTER TABLE {schema}.good OWNER TO {owner};
                 COMMENT ON TABLE {schema}.good IS 'tetsu:global: test';
                 CREATE POLICY wide ON {schema}.good TO {group} USING (true);
                 CREATE TYPE {schema}.mood AS ENUM ('calm'); ALTER TYPE {schema}.mood OWNER TO {app}"
            ))
            .await?;
        tetsu_postgres::isolation_violations_for(&mut client, &[&schema], &app).await
    }
    .await;
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE;
             DROP ROLE IF EXISTS {app}, {owner}, {hidden}, {group}, {module}"
        ))
        .await
        .expect("drops the roles");
    let found = checked.expect("checks");
    for expected in [
        format!("{module} belongs to {hidden}, but module and lookup roles belong to nothing"),
        format!("{schema}.good has policy wide for its owner, {owner}"),
        format!("{app} owns type {schema}.mood"),
    ] {
        assert!(found.contains(&expected), "{expected} among {found:?}");
    }
}

/// Each case is one migration into schema `checked`, with `checked_other` beside it, and the problem it must show.
async fn each_caught(cases: &[(&str, &str, &str)]) {
    for (sql, other, expected) in cases {
        let found = problems((*sql).to_string(), other.to_string().leak()).await;
        assert!(
            found.iter().any(|problem| problem.contains(expected)),
            "expected {expected:?} among {found:?}"
        );
    }
}

#[tokio::test]
async fn foreign_keys_must_match_tenant_to_tenant() {
    let parent = tenant_owned("parent");
    let child = tenant_owned("child");
    each_caught(&[
        (
            &format!(
                "{parent} ALTER TABLE parent ADD COLUMN seq bigint NOT NULL DEFAULT 0, ADD UNIQUE (tenant_id, seq);
                 {child} ALTER TABLE child ADD COLUMN other bigint,
                   ADD CONSTRAINT crossed FOREIGN KEY (other, tenant_id) REFERENCES parent (tenant_id, seq);"
            ),
            "",
            "checked.child's foreign key crossed pairs tenant_id with another column",
        ),
        (
            &format!(
                "{parent} CREATE TABLE note (parent uuid, tenant_id bigint, id int PRIMARY KEY);
                 COMMENT ON TABLE note IS 'tetsu:global: test';
                 ALTER TABLE note ADD CONSTRAINT to_parent FOREIGN KEY (tenant_id, parent)
                   REFERENCES parent (tenant_id, id);"
            ),
            "",
            "checked.note's foreign key to_parent leaves out tenant_id",
        ),
    ])
    .await;
}

#[tokio::test]
async fn functions_that_run_early_or_pick_a_tenant_are_caught() {
    let good = tenant_owned("good");
    each_caught(&[
        (
            "CREATE FUNCTION peek(text) RETURNS boolean LANGUAGE sql LEAKPROOF AS 'SELECT true'",
            "",
            "function checked.peek(text) is leakproof, so it runs before row-level security",
        ),
        (
            &format!(
                "{good} CREATE FUNCTION count_two() RETURNS bigint LANGUAGE sql SET app.tenant_id = '2'
                   AS 'SELECT count(*) FROM checked.good'"
            ),
            "",
            "function checked.count_two() sets app.tenant_id",
        ),
        (
            &format!(
                "{good} CREATE TRIGGER crossing AFTER INSERT ON good FOR EACH ROW
                   EXECUTE FUNCTION checked_other.hook()"
            ),
            "CREATE FUNCTION hook() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NULL; END'",
            "checked.good uses function checked_other.hook() from another schema",
        ),
        (
            &format!(
                "{good} CREATE FUNCTION mine() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER
                   SET search_path = pg_catalog, pg_temp AS 'BEGIN RETURN NULL; END';
                 ALTER FUNCTION mine() OWNER TO tetsu_checked_lookup;
                 CREATE TRIGGER as_owner AFTER INSERT ON good FOR EACH ROW EXECUTE FUNCTION mine()"
            ),
            "",
            "trigger as_owner on checked.good runs a function as its owner",
        ),
    ])
    .await;
}

#[tokio::test]
async fn grant_options_defaults_and_odd_declarations_are_caught() {
    let good = tenant_owned("good");
    each_caught(&[
        (
            &format!("{good} GRANT SELECT ON good TO tetsu_checked WITH GRANT OPTION;"),
            "",
            "tetsu_checked holds SELECT on checked.good, with the right to grant it",
        ),
        (
            "ALTER DEFAULT PRIVILEGES IN SCHEMA checked GRANT SELECT ON TABLES TO PUBLIC",
            "",
            "PUBLIC is named in default privileges of postgres",
        ),
        (
            "ALTER DEFAULT PRIVILEGES GRANT SELECT ON TABLES TO tetsu_checked_other",
            "",
            "tetsu_checked_other is named in default privileges of postgres",
        ),
        (
            &good.replace("tenant_id bigint", "tenant_id smallint"),
            "",
            "checked.good's tenant_id is not a bigint",
        ),
        (
            "CREATE TABLE parts (tenant_id bigint NOT NULL, id uuid NOT NULL, PRIMARY KEY (tenant_id, id))
               PARTITION BY HASH (tenant_id);
             COMMENT ON TABLE parts IS 'tetsu:tenant-owned';
             CREATE TABLE parts_0 PARTITION OF parts FOR VALUES WITH (MODULUS 1, REMAINDER 0);
             COMMENT ON TABLE parts_0 IS 'tetsu:global: test'",
            "",
            "checked.parts_0 inherits from tenant-owned checked.parts but is not declared tenant-owned",
        ),
        (
            "CREATE TABLE vague (id int); COMMENT ON TABLE vague IS 'tetsu:global:   '",
            "",
            "checked.vague is declared neither tenant-owned nor global",
        ),
        (
            &format!(
                "{good} CREATE POLICY loose ON good TO tetsu_checked USING (true);"
            ),
            "",
            "checked.good has policy loose that is not the tenant rule for tetsu_checked alone",
        ),
    ])
    .await;
}

#[tokio::test]
async fn ownership_of_every_kind_of_object_is_checked() {
    let good = tenant_owned("good");
    each_caught(&[
        (
            "CREATE FUNCTION plain() RETURNS int LANGUAGE sql AS 'SELECT 1';
             ALTER FUNCTION plain() OWNER TO tetsu_checked",
            "",
            "function checked.plain() is owned by tetsu_checked",
        ),
        (
            "CREATE TYPE mood AS ENUM ('calm'); ALTER TYPE mood OWNER TO tetsu_checked",
            "",
            "type checked.mood is owned by tetsu_checked",
        ),
        (
            &format!(
                "{good} CREATE STATISTICS spread ON tenant_id, id FROM good;
                 ALTER STATISTICS spread OWNER TO tetsu_checked"
            ),
            "",
            "statistics spread is owned by tetsu_checked",
        ),
        (
            "CREATE COLLATION plainly (provider = icu, locale = 'und');
             ALTER COLLATION plainly OWNER TO tetsu_checked",
            "",
            "collation plainly is owned by tetsu_checked",
        ),
        (
            "CREATE OPERATOR === (LEFTARG = int, RIGHTARG = int, FUNCTION = int4eq);
             ALTER OPERATOR === (int, int) OWNER TO tetsu_checked",
            "",
            "is owned by tetsu_checked",
        ),
    ])
    .await;
}

#[tokio::test]
async fn database_wide_objects_tetsu_never_uses_are_caught() {
    each_caught(&[
        (
            "CREATE EXTENSION btree_gist SCHEMA public",
            "",
            "extension btree_gist is installed, and no rule covers it",
        ),
        (
            "CREATE FUNCTION pg_catalog.zz_sneak() RETURNS int LANGUAGE sql AS 'SELECT 1'",
            "",
            "zz_sneak() lies outside every module's schema",
        ),
        (
            "CREATE TABLE public.dump (id int)",
            "",
            "public.dump lies outside every module's schema",
        ),
        (
            "SELECT lo_create(0)",
            "",
            "exists, which row-level security does not reach",
        ),
        (
            "DO $$ BEGIN EXECUTE format('GRANT CREATE ON DATABASE %I TO tetsu_app', current_database()); END $$",
            "",
            "tetsu_app holds CREATE on database",
        ),
        (
            "DO $$ BEGIN EXECUTE format('GRANT TEMPORARY ON DATABASE %I TO PUBLIC', current_database()); END $$",
            "",
            "PUBLIC holds TEMPORARY on database",
        ),
    ])
    .await;
}

#[tokio::test]
async fn features_that_reach_past_row_level_security_are_caught() {
    each_caught(&[
        (
            "CREATE FUNCTION checked.on_ddl() RETURNS event_trigger LANGUAGE plpgsql AS 'BEGIN END';
             CREATE EVENT TRIGGER watcher ON ddl_command_start EXECUTE FUNCTION checked.on_ddl()",
            "",
            "event trigger watcher runs on every statement",
        ),
        (
            "CREATE PUBLICATION everything FOR ALL TABLES",
            "",
            "publication everything copies rows out of the database",
        ),
        (
            "CREATE FOREIGN DATA WRAPPER nowhere; CREATE SERVER far FOREIGN DATA WRAPPER nowhere;
             CREATE FOREIGN TABLE elsewhere (id int) SERVER far",
            "",
            "checked.elsewhere is a kind of relation a module schema does not hold",
        ),
        (
            "CREATE TRUSTED LANGUAGE extra HANDLER plpgsql_call_handler",
            "",
            "language extra was added",
        ),
        (
            "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I SET search_path = checked, pg_catalog', current_database()); END $$",
            "",
            "database settings for every role set search_path",
        ),
    ])
    .await;
}

#[tokio::test]
async fn a_composite_type_or_the_checker_s_temp_table_is_no_problem() {
    assert_eq!(
        problems("CREATE TYPE pair AS (a int, b int)".into(), "").await,
        Vec::<String>::new()
    );
    let database = fresh_database().await.expect("a test database");
    migrate(&database, &[&support::Checked("checked", &[])])
        .await
        .expect("migrates");
    let mut client = connect(&database).await.expect("connects");
    client
        .batch_execute("CREATE TEMP TABLE scratch (id int)")
        .await
        .expect("makes a temp table");
    assert_eq!(
        isolation_violations(&mut client, &["checked", "missing"])
            .await
            .expect("checks"),
        ["schema missing does not exist"]
    );
}

/// Server-wide grants, checked against roles and a module of the test's own, so no shared role is touched.
#[tokio::test]
async fn reach_logins_parameters_and_owners_that_can_skip_rls_are_caught() {
    let database = fresh_database().await.expect("a test database");
    let id = std::process::id();
    let (app, owner, bypass) = (
        format!("t_iso4_app_{id}"),
        format!("t_iso4_owner_{id}"),
        format!("t_iso4_bypass_{id}"),
    );
    let schema = format!("iso4_{id}");
    let (module, lookup) = (format!("tetsu_{schema}"), format!("tetsu_{schema}_lookup"));
    let mut client = connect(&database).await.expect("connects");
    let checked = async {
        client
            .batch_execute(&format!(
                "CREATE ROLE {app}; CREATE ROLE {owner}; CREATE ROLE {bypass} BYPASSRLS;
                 CREATE ROLE {module} LOGIN; CREATE ROLE {lookup};
                 GRANT {module}, {lookup}, {owner} TO {app} WITH INHERIT FALSE, SET TRUE;
                 GRANT {bypass} TO {owner} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE;
                 GRANT SET ON PARAMETER session_replication_role TO {app};
                 CREATE SCHEMA {schema} AUTHORIZATION {owner}"
            ))
            .await?;
        tetsu_postgres::isolation_violations_for(&mut client, &[&schema], &app).await
    }
    .await;
    client
        .batch_execute(&format!(
            "REVOKE SET ON PARAMETER session_replication_role FROM {app};
             DROP SCHEMA IF EXISTS {schema} CASCADE;
             DROP ROLE IF EXISTS {app}, {owner}, {bypass}, {module}, {lookup}"
        ))
        .await
        .expect("drops the roles");
    let found = checked.expect("checks");
    for expected in [
        format!("{app} can act as {lookup}, which is no module's role"),
        format!("{app} can act as {owner}, which is no module's role"),
        format!("{module} can log in, but a module or lookup role never does"),
        format!("{app} may set parameter session_replication_role"),
        format!("the owner {owner} can become {bypass}, which skips row-level security"),
    ] {
        assert!(found.contains(&expected), "{expected} among {found:?}");
    }
}

/// The shared outbox migration, which grants the delivery role exactly what it needs, passes.
#[tokio::test]
async fn the_shared_outbox_passes() {
    assert_eq!(
        problems(tetsu_postgres::OUTBOX.to_string(), "").await,
        Vec::<String>::new()
    );
}

/// The shared inbox migrations, with the delivery role's right to remove IDs past their use (ADR-0080), pass.
#[tokio::test]
async fn the_shared_inbox_passes() {
    let inbox = format!(
        "{}\n{}",
        tetsu_postgres::INBOX,
        tetsu_postgres::INBOX_DELIVERY
    );
    assert_eq!(problems(inbox, "").await, Vec::<String>::new());
}

/// The delivery role only reads and removes inbox IDs, so a right to add or change one is caught.
#[tokio::test]
async fn the_delivery_role_never_writes_an_inbox() {
    let inbox = format!(
        "{}\n{}",
        tetsu_postgres::INBOX,
        tetsu_postgres::INBOX_DELIVERY
    );
    let with = |extra: &str| format!("{inbox}\n{extra}");
    each_caught(&[
        (
            &with("GRANT INSERT ON inbox_global TO tetsu_delivery"),
            "",
            "tetsu_delivery holds INSERT on checked.inbox_global",
        ),
        (
            &with("GRANT UPDATE ON inbox TO tetsu_delivery"),
            "",
            "tetsu_delivery holds UPDATE on checked.inbox",
        ),
        (
            &with("CREATE POLICY narrow ON inbox TO tetsu_delivery USING (tenant_id > 0)"),
            "",
            "checked.inbox has policy narrow that is not the tenant rule",
        ),
    ])
    .await;
}

/// Only the shared inbox's exact shape opens to the delivery role, so a module's own table named `inbox`, such as
/// customers' messages, stays closed to it.
#[tokio::test]
async fn a_table_named_inbox_is_not_the_shared_inbox() {
    let own = tenant_owned("inbox");
    let shaped = |name: &str, columns: &str| {
        format!("CREATE TABLE {name} ({columns}); GRANT SELECT, DELETE ON {name} TO tetsu_delivery")
    };
    let (extra, reordered, other) = (
        shaped(
            "inbox_global",
            "id uuid PRIMARY KEY, applied_at timestamptz, note text",
        ),
        shaped(
            "inbox_global",
            "applied_at timestamptz, id uuid PRIMARY KEY",
        ),
        shaped("inbox_global", "id uuid PRIMARY KEY, note text"),
    );
    let global = "tetsu_delivery holds SELECT on checked.inbox_global";
    each_caught(&[
        (&extra, "", global),
        (&reordered, "", global),
        (&other, "", global),
        (
            &format!("{own}\nGRANT SELECT, DELETE ON inbox TO tetsu_delivery"),
            "",
            "tetsu_delivery holds SELECT on checked.inbox",
        ),
        (
            &format!("{own}\nCREATE POLICY delivery ON inbox TO tetsu_delivery USING (true)"),
            "",
            "checked.inbox has policy delivery that is not the tenant rule",
        ),
    ])
    .await;
}

/// The delivery role reads and settles outboxes across tenants and nothing more, so anything wider is caught.
#[tokio::test]
async fn the_delivery_role_reaches_outboxes_alone() {
    let outbox = tetsu_postgres::OUTBOX;
    let with = |extra: &str| format!("{outbox}\n{extra}");
    let other = tenant_owned("other");
    each_caught(&[
        (
            &format!("{other}\nGRANT SELECT ON other TO tetsu_delivery"),
            "",
            "tetsu_delivery holds SELECT on checked.other",
        ),
        (
            &with("GRANT INSERT ON outbox TO tetsu_delivery"),
            "",
            "tetsu_delivery holds INSERT on checked.outbox",
        ),
        (
            &with("GRANT UPDATE ON outbox TO tetsu_delivery"),
            "",
            "tetsu_delivery holds UPDATE on checked.outbox",
        ),
        (
            &with("GRANT UPDATE (tenant_id) ON outbox TO tetsu_delivery"),
            "",
            "tetsu_delivery holds UPDATE on checked.outbox (column tenant_id)",
        ),
        (
            &with("GRANT SELECT ON outbox TO tetsu_delivery WITH GRANT OPTION"),
            "",
            "with the right to grant it",
        ),
        (
            &format!("{other}\nCREATE POLICY delivery ON other TO tetsu_delivery USING (true)"),
            "",
            "checked.other has policy delivery that is not the tenant rule",
        ),
        (
            &with("CREATE POLICY narrow ON outbox TO tetsu_delivery USING (tenant_id > 0)"),
            "",
            "checked.outbox has policy narrow that is not the tenant rule",
        ),
    ])
    .await;
}

/// The database bounds what Rust bounds: a text, binary, or JSON column needs a validated check on its size, or one
/// holding it to listed values (ADR-0060).
#[tokio::test]
async fn columns_without_a_size_check_are_caught() {
    let global = "COMMENT ON TABLE notes IS 'tetsu:global: test';";
    for (column, flagged) in [
        ("body text", true),
        ("body varchar", true),
        ("body bytea", true),
        ("body jsonb", true),
        ("body json", true),
        ("body varchar(20)", false),
        ("body text CHECK (body <> '')", true),
        ("body text CHECK (octet_length(body) <= 100)", false),
        ("body text CHECK (char_length(body) <= 100)", false),
        ("body text CHECK (length(body) <= 100)", false),
        ("body jsonb CHECK (octet_length(body::text) <= 100)", false),
        ("body text CHECK (body IN ('a', 'b'))", false),
        ("body text CHECK (body = 'a')", false),
        (
            "body text, other text CHECK (octet_length(other) <= 9)",
            true,
        ),
        (
            "body text); ALTER TABLE notes ADD CHECK (octet_length(body) <= 9) NOT VALID; SELECT (1",
            true,
        ),
        // A lower bound alone, a bound on another column, and a comparison that is not equality bound nothing.
        ("body text CHECK (char_length(body) > 0)", true),
        ("body text CHECK (char_length(body) BETWEEN 1 AND 9)", false),
        ("body text CHECK (octet_length(body) = 7)", false),
        (
            "body text, kind text, CHECK (body <> '' AND kind = 'x')",
            true,
        ),
        (
            "body text, kind text, CHECK (octet_length(kind) <= 9 AND body <> '')",
            true,
        ),
        ("body text CHECK (body >= 'a')", true),
        ("somebody text, body text CHECK (somebody = 'a')", true),
        ("body bpchar", true),
        ("body char(5)", false),
    ] {
        let found = problems(format!("CREATE TABLE notes ({column}); {global}"), "").await;
        assert_eq!(
            found.contains(&"checked.notes.body has no check on its size".to_string()),
            flagged,
            "{column}: {found:?}"
        );
    }
}

/// A key holding a column that can change, such as a place in a list, or an id that is not a uuid (ADR-0060).
#[tokio::test]
async fn keys_other_than_the_tenant_and_a_uuid_id_are_caught() {
    caught(
        format!(
            "{} ALTER TABLE good ADD COLUMN place int NOT NULL,
                 DROP CONSTRAINT good_pkey, ADD PRIMARY KEY (tenant_id, place);",
            tenant_owned("good")
        ),
        "checked.good's primary key is not (tenant_id, id) with a uuid id",
    )
    .await;
    caught(
        format!(
            "{} ALTER TABLE good ADD COLUMN place int NOT NULL,
                 DROP CONSTRAINT good_pkey, ADD PRIMARY KEY (tenant_id, id, place);",
            tenant_owned("good")
        ),
        "checked.good's primary key is not (tenant_id, id) with a uuid id",
    )
    .await;
    caught(
        "CREATE TABLE odd (tenant_id bigint NOT NULL, id bigint NOT NULL, PRIMARY KEY (tenant_id, id));
         COMMENT ON TABLE odd IS 'tetsu:tenant-owned'"
            .into(),
        "checked.odd's primary key is not (tenant_id, id) with a uuid id",
    )
    .await;
}

/// A domain over text needs a check on its size as text does, and an array of text, binary, or JSON has no size a
/// check can show, so both are caught (ADR-0060).
#[tokio::test]
async fn domains_and_arrays_of_text_are_caught() {
    caught(
        "CREATE DOMAIN words AS text; CREATE TABLE notes (body words);
         COMMENT ON TABLE notes IS 'tetsu:global: test';"
            .into(),
        "checked.notes.body has no check on its size",
    )
    .await;
    caught(
        "CREATE TABLE notes (tags text[]); COMMENT ON TABLE notes IS 'tetsu:global: test';".into(),
        "checked.notes.tags is an array of text, binary, or JSON, which no size rule covers",
    )
    .await;
}
