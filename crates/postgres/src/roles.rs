//! Tetsu's own roles (ADR-0042). Roles belong to the whole server, not one database, so every step here can find
//! its work already done, or being done by another connection at the same moment.

use std::time::Duration;

use tokio_postgres::Client;
use tokio_postgres::error::SqlState;

use crate::connection::Error;

/// The role `tetsu serve` signs in as. It holds no rights of its own and acts through module roles.
pub const APP_ROLE: &str = "tetsu_app";
/// The role the dispatcher acts as: it reads and settles every tenant's outbox rows, and nothing else (ADR-0043).
pub const DELIVERY_ROLE: &str = "tetsu_delivery";
/// Module roles are `tetsu_<schema>` and `tetsu_<schema>_lookup`, so a schema leaves room for both in Postgres's
/// 63-byte names.
pub const MAX_SCHEMA: usize = 63 - "tetsu_".len() - "_lookup".len();
const RETRIES: u32 = 5;
const RETRY_PAUSE: Duration = Duration::from_millis(50);

/// The role a module's code acts as, holding its rights on its own schema alone.
#[must_use]
pub fn module_role(schema: &str) -> String {
    format!("tetsu_{schema}")
}

/// The role that owns a module's lookup functions, which run before a tenant is known.
#[must_use]
pub fn lookup_role(schema: &str) -> String {
    format!("tetsu_{schema}_lookup")
}

/// Runs `create`, which makes `role`, unless the role exists, so a runner that cannot create roles still migrates
/// once an administrator has made them. Two connections creating one role at once may collide in the catalog, so a
/// collision is retried.
///
/// # Errors
/// Fails if the statement fails for any other reason, or keeps colliding.
pub async fn ensure_role(client: &Client, role: &str, create: &str) -> Result<(), Error> {
    if exists(client, role).await? {
        return Ok(());
    }
    let statement = format!(
        "DO $$ BEGIN {create}; EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$"
    );
    retried(client, &statement).await
}

async fn exists(client: &Client, role: &str) -> Result<bool, Error> {
    let row = client
        .query_one(
            "SELECT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = $1) AS found",
            &[&role],
        )
        .await?;
    Ok(row.try_get("found")?)
}

/// Grants `role` to `member` to act as, never to inherit, unless that grant already stands.
async fn ensure_member(client: &Client, role: &str, member: &str) -> Result<(), Error> {
    let row = client
        .query_one(
            "SELECT EXISTS (SELECT FROM pg_catalog.pg_auth_members m
               JOIN pg_catalog.pg_roles r ON r.oid = m.roleid JOIN pg_catalog.pg_roles u ON u.oid = m.member
               WHERE r.rolname = $1 AND u.rolname = $2 AND m.set_option AND NOT m.inherit_option) AS found",
            &[&role, &member],
        )
        .await?;
    if row.try_get("found")? {
        return Ok(());
    }
    retried(
        client,
        &format!(
            "GRANT {role} TO {} WITH INHERIT FALSE, SET TRUE",
            quoted(member)
        ),
    )
    .await
}

/// Makes a module's roles, lets the application role and the runner act as its module role one transaction at a
/// time, and lets the runner hand functions to its lookup role. A grant that already exists is kept. It also makes
/// the delivery role, which a module's outbox file grants to.
pub(crate) async fn prepare_module(client: &Client, schema: &str) -> Result<(), Error> {
    let (module, lookup) = (module_role(schema), lookup_role(schema));
    for role in [&module, &lookup, &DELIVERY_ROLE.to_string()] {
        ensure_role(client, role, &format!("CREATE ROLE {role} NOLOGIN")).await?;
    }
    let runner: String = client
        .query_one("SELECT current_user::text AS runner", &[])
        .await?
        .try_get("runner")?;
    ensure_member(client, &module, APP_ROLE).await?;
    ensure_member(client, DELIVERY_ROLE, APP_ROLE).await?;
    ensure_member(client, &module, &runner).await?;
    ensure_member(client, &lookup, &runner).await?;
    // These belong to this database alone, so nothing else races them. Functions are revoked from `PUBLIC` in the
    // global form, since a schema's own defaults cannot take back a global grant.
    client
        .batch_execute(&format!(
            r#"GRANT USAGE ON SCHEMA "{schema}" TO {module}, {lookup};
             ALTER DEFAULT PRIVILEGES IN SCHEMA "{schema}" GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {module};
             ALTER DEFAULT PRIVILEGES IN SCHEMA "{schema}" GRANT USAGE, SELECT ON SEQUENCES TO {module};
             ALTER DEFAULT PRIVILEGES REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC"#
        ))
        .await?;
    Ok(())
}

/// A name as a quoted identifier, so no name can end the statement or change its case.
pub(crate) fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Fails unless the runner, whose role owns everything a migration makes, is bound by row-level security. A
/// superuser is allowed, as on a local test server, since managed hosts never hand one out; a role that skips
/// row-level security without being one, as a host's administrative roles do, is refused (ADR-0042).
pub(crate) async fn require_bound_runner(client: &Client) -> Result<(), Error> {
    let row = client
        .query_one(
            "SELECT rolname, rolsuper, rolbypassrls FROM pg_catalog.pg_roles WHERE rolname = current_user",
            &[],
        )
        .await?;
    let (name, superuser, bypasses): (String, bool, bool) =
        (row.try_get(0)?, row.try_get(1)?, row.try_get(2)?);
    // A role it can become counts too, through SET or by granting itself a role it may administer, however many
    // grants away, since a migration file could do either.
    let reaches = client
        .query_one(
            "WITH RECURSIVE reach (oid) AS (
               SELECT oid FROM pg_catalog.pg_roles WHERE rolname = current_user
               UNION
               SELECT m.roleid FROM reach r JOIN pg_catalog.pg_auth_members m ON m.member = r.oid
               WHERE m.set_option OR m.admin_option
             )
             SELECT EXISTS (SELECT FROM reach JOIN pg_catalog.pg_roles r ON r.oid = reach.oid
               WHERE r.rolname <> current_user AND (r.rolbypassrls OR r.rolsuper)) AS reaches",
            &[],
        )
        .await?
        .try_get::<_, bool>("reaches")?;
    if !superuser && (bypasses || reaches) {
        return Err(format!(
            "role {name} skips row-level security, or can become a role that does, so it must not run migrations; \
             migrate as Tetsu's own owner role"
        )
        .into());
    }
    Ok(())
}

/// Fails unless `role` exists, such as the application role, which the runner grants each module's role.
pub(crate) async fn require_role(client: &Client, role: &str) -> Result<(), Error> {
    if exists(client, role).await? {
        Ok(())
    } else {
        Err(format!(
            "role {role} must exist before migrating; create it as docs/operations/demo-environment.md shows"
        )
        .into())
    }
}

/// Runs `statement`, and again after each collision, up to `RETRIES` more times with a longer pause each time.
async fn retried(client: &Client, statement: &str) -> Result<(), Error> {
    for attempt in 1..=RETRIES {
        match client.batch_execute(statement).await {
            Ok(()) => return Ok(()),
            Err(error) if collided(&error) => tokio::time::sleep(RETRY_PAUSE * attempt).await,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(client.batch_execute(statement).await?)
}

/// Another connection changed the same catalog row first: "tuple concurrently updated" is an internal error, and a
/// role or grant made at the same moment can break a unique index.
fn collided(error: &tokio_postgres::Error) -> bool {
    error.code().is_some_and(|code| {
        [
            SqlState::INTERNAL_ERROR,
            SqlState::UNIQUE_VIOLATION,
            SqlState::T_R_SERIALIZATION_FAILURE,
        ]
        .contains(code)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `failing` through `retried`, counting attempts with a sequence, which a failed transaction does not roll
    /// back, and timing them.
    async fn attempts(code: &str) -> (u64, Duration, String) {
        let database = crate::test_support::fresh_database()
            .await
            .expect("a test database");
        let client = crate::connect(&database).await.expect("connects");
        client
            .batch_execute("CREATE SEQUENCE tries")
            .await
            .expect("creates the counter");
        let started = std::time::Instant::now();
        let error = retried(
            &client,
            &format!("SELECT nextval('tries'); DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = '{code}'; END $$"),
        )
        .await
        .expect_err("keeps failing");
        let elapsed = started.elapsed();
        let tries: i64 = client
            .query_one("SELECT last_value FROM tries", &[])
            .await
            .expect("reads the counter")
            .try_get(0)
            .expect("a count");
        (tries.unsigned_abs(), elapsed, error.to_string())
    }

    #[tokio::test]
    async fn a_collision_is_retried_with_growing_pauses_then_given_up() {
        for code in ["XX000", "23505", "40001"] {
            let (tries, elapsed, _) = attempts(code).await;
            assert_eq!(tries, u64::from(RETRIES) + 1, "{code}");
            // 50, 100, 150, 200, and 250 milliseconds.
            assert!(elapsed >= Duration::from_millis(750), "{code}: {elapsed:?}");
        }
    }

    #[tokio::test]
    async fn a_missing_role_is_named() {
        let database = crate::test_support::fresh_database()
            .await
            .expect("a test database");
        let client = crate::connect(&database).await.expect("connects");
        require_role(&client, APP_ROLE)
            .await
            .expect("the app role exists");
        let error = require_role(&client, "t_no_such_role")
            .await
            .expect_err("refuses")
            .to_string();
        assert_eq!(
            error,
            "role t_no_such_role must exist before migrating; create it as docs/operations/demo-environment.md shows"
        );
    }

    #[tokio::test]
    async fn any_other_error_is_not_retried() {
        let (tries, _, error) = attempts("42501").await;
        assert_eq!(tries, 1);
        assert!(error.contains("db error"), "{error}");
    }

    #[test]
    fn a_module_s_roles_fit_postgres_names() {
        let schema = "a".repeat(MAX_SCHEMA);
        assert_eq!(module_role("tenancy"), "tetsu_tenancy");
        assert_eq!(lookup_role("tenancy"), "tetsu_tenancy_lookup");
        assert_eq!(lookup_role(&schema).len(), 63);
    }
}
