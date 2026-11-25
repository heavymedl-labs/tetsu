//! Staff joining a business by an invite, in Postgres as the application role (ADR-0089, ADR-0091): the invite is
//! found by its link before its business is known, and joining by it or accepting it uses it up and adds its role in
//! the business's own transaction, so the check and the writes commit together.

use tetsu_identity::Identity;
use tetsu_postgres::test_support::{TestDatabase, migrated_database};
use tetsu_postgres::{ModuleTransaction, Pool, TenantTransaction, connect, pool};
use tokio_postgres::types::ToSql;

/// The business the invites in these tests belong to.
const BUSINESS: i64 = 3;

#[cfg(test)]
mod support {
    use super::*;

    /// Runs SQL as the test server's database administrator, to set the scene.
    pub async fn as_db_admin(database: &TestDatabase, sql: &str) {
        connect(database)
            .await
            .expect("connects")
            .batch_execute(sql)
            .await
            .expect("runs");
    }

    /// A number the database administrator reads, such as how many passkeys are stored.
    pub async fn count(database: &TestDatabase, sql: &str) -> i64 {
        connect(database)
            .await
            .expect("connects")
            .query_one(sql, &[])
            .await
            .expect("reads")
            .try_get(0)
            .expect("a number")
    }

    /// Ada Park, invited to the business as its owner by the link `ada`, and Bo Lim, invited by `bo`; neither has a
    /// passkey yet. An admin, Kurt, exists too, so a test can show staff's functions leave admins alone.
    pub async fn invited() -> (TestDatabase, Pool) {
        let database = migrated_database(&[&Identity])
            .await
            .expect("a migrated test database");
        as_db_admin(
            &database,
            &format!(
                "INSERT INTO identity.admins (name) VALUES ('Kurt Medley');
                 INSERT INTO identity.staff (name, email)
                   VALUES ('Ada Park', 'ada@example.com'), ('Bo Lim', 'bo@example.com');
                 INSERT INTO identity.staff_invites
                   (tenant_id, token_hash, staff_id, role_name, issued_at, ends_at)
                   SELECT {BUSINESS}, sha256(lower(split_part(name, ' ', 1))::bytea), id, 'owner', now(),
                     now() + interval '7 days'
                   FROM identity.staff"
            ),
        )
        .await;
        let pool = pool(&database.as_app()).expect("a pool");
        (database, pool)
    }

    /// The hash of a link or token, as Identity stores it.
    pub fn hash(text: &str) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        Sha256::digest(text.as_bytes()).to_vec()
    }

    /// A staff member's ID by their first name.
    pub async fn staff(database: &TestDatabase, first: &str) -> i64 {
        count(
            database,
            &format!("SELECT id FROM identity.staff WHERE name LIKE '{first} %'"),
        )
        .await
    }

    /// Who an invite's link names, as the page reads it: their business, ID, name, role, and whether they joined.
    pub async fn invitee(pool: &Pool, link: &str) -> Option<(i64, i64, String, String, bool)> {
        let mut client = pool.get().await.expect("a connection");
        let transaction = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        let row = transaction
            .query_opt(
                "SELECT tenant_id, staff_id, name, role_name, joined FROM identity.staff_invitee($1)",
                &[&hash(link)],
            )
            .await
            .expect("reads")?;
        Some((
            row.try_get(0).expect("a business"),
            row.try_get(1).expect("a staff member"),
            row.try_get(2).expect("a name"),
            row.try_get(3).expect("a role"),
            row.try_get(4).expect("joined"),
        ))
    }

    /// Runs one statement that answers with text in a business's own transaction, and commits whatever it did.
    pub async fn in_business(
        pool: &Pool,
        business: i64,
        sql: &str,
        parameters: &[&(dyn ToSql + Sync)],
    ) -> String {
        let mut client = pool.get().await.expect("a connection");
        let transaction = TenantTransaction::begin(&mut client, "identity", business)
            .await
            .expect("begins");
        let outcome: Option<String> = transaction
            .query_one(sql, parameters)
            .await
            .expect("answers")
            .try_get(0)
            .expect("an outcome");
        transaction.commit().await.expect("commits");
        outcome.unwrap_or_else(|| "unknown".to_string())
    }

    /// Joins a staff member by a link in a business, as staff's caller does, ending the session `presented` names.
    pub async fn join(
        pool: &Pool,
        business: i64,
        link: &str,
        staff: i64,
        credential: &[u8],
    ) -> String {
        in_business(
            pool,
            business,
            "SELECT identity.staff_join($1, $2, $3, '{}'::jsonb, '', $4, $5)",
            &[
                &hash(link),
                &staff,
                &credential,
                &hash(&format!("{link} {credential:?}")),
                &hash("presented"),
            ],
        )
        .await
    }

    /// Accepts a link in a business for the session a token names.
    pub async fn accept(pool: &Pool, business: i64, link: &str, session: &str) -> String {
        in_business(
            pool,
            business,
            "SELECT identity.staff_accept_invite($1, $2)",
            &[&hash(link), &hash(session)],
        )
        .await
    }

    /// Gives a staff member a passkey and four sessions, as the database administrator: `fresh` proven now, `stale`
    /// proven twenty minutes ago, `unproven` never proven, and `ended` past its absolute limit.
    pub async fn logged_in(database: &TestDatabase, staff: i64) {
        as_db_admin(
            database,
            &format!(
                "INSERT INTO identity.staff_passkeys (credential_id, staff_id, passkey, created_at)
                   VALUES (sha256('{staff}'::bytea), {staff}, '{{}}', now());
                 INSERT INTO identity.staff_sessions
                   (token_hash, realm, staff_id, started_at, proven_at, last_seen_at, ends_at)
                 VALUES
                   (sha256('fresh'::bytea), 'staff', {staff}, now(), now(), now(), now() + interval '1 hour'),
                   (sha256('stale'::bytea), 'staff', {staff}, now(), now() - interval '20 minutes', now(),
                     now() + interval '1 hour'),
                   (sha256('unproven'::bytea), 'staff', {staff}, now(), NULL, now(), now() + interval '1 hour'),
                   (sha256('ended'::bytea), 'staff', {staff}, now() - interval '13 hours', now(), now(),
                     now() - interval '1 hour')"
            ),
        )
        .await;
    }

    /// What is stored: staff passkeys, staff sessions, open invites, owners at the business, and admins' sessions
    /// and passkeys.
    pub async fn stored(database: &TestDatabase) -> [i64; 6] {
        let mut counts = [0; 6];
        for (slot, sql) in counts.iter_mut().zip([
            "SELECT count(*) FROM identity.staff_passkeys",
            "SELECT count(*) FROM identity.staff_sessions",
            "SELECT count(*) FROM identity.staff_invites",
            "SELECT count(*) FROM identity.staff_roles WHERE tenant_id = 3 AND role_name = 'owner'",
            "SELECT count(*) FROM identity.admin_sessions",
            "SELECT count(*) FROM identity.passkeys",
        ]) {
            *slot = count(database, sql).await;
        }
        counts
    }

    /// Begins a join ceremony for a staff member by a link, and says whether it began.
    pub async fn begin(pool: &Pool, staff: i64, link: &str) -> bool {
        let mut client = pool.get().await.expect("a connection");
        let transaction = ModuleTransaction::begin(&mut client, "identity")
            .await
            .expect("begins");
        let begun = transaction
            .query_one(
                "SELECT identity.staff_begin_join($1, $2, $3, '{}'::jsonb)",
                &[&hash(&format!("{link} ceremony")), &staff, &hash(link)],
            )
            .await
            .expect("answers")
            .try_get(0)
            .expect("begun");
        transaction.commit().await.expect("commits");
        begun
    }
}

use support::*;

/// A link names its business, person, and role, and whether they joined; nothing for a link that ended, names no
/// one, or names someone disabled.
#[tokio::test]
async fn an_invite_is_found_by_its_link_alone() {
    let (database, pool) = invited().await;
    let ada = staff(&database, "Ada").await;
    assert_eq!(
        invitee(&pool, "ada").await,
        Some((
            BUSINESS,
            ada,
            "Ada Park".to_string(),
            "owner".to_string(),
            false
        ))
    );
    logged_in(&database, ada).await;
    assert_eq!(invitee(&pool, "ada").await.map(|found| found.4), Some(true));
    assert_eq!(invitee(&pool, "nobody").await, None);
    as_db_admin(
        &database,
        "UPDATE identity.staff SET disabled_at = now() WHERE name = 'Bo Lim'",
    )
    .await;
    assert_eq!(invitee(&pool, "bo").await, None);
    as_db_admin(
        &database,
        "UPDATE identity.staff_invites SET ends_at = now() - interval '1 second'",
    )
    .await;
    assert_eq!(invitee(&pool, "ada").await, None);
}

/// A new person joins by their link: their passkey and an unproven session are stored, the session the browser
/// presented ends, the invite is used up, and its role is the business's; admins are left alone. Finishing the same
/// link again finds them joined, and changes nothing.
#[tokio::test]
async fn a_new_person_joins_by_their_link() {
    let (database, pool) = invited().await;
    let ada = staff(&database, "Ada").await;
    as_db_admin(
        &database,
        &format!(
            "INSERT INTO identity.staff_sessions (token_hash, realm, staff_id, started_at, last_seen_at, ends_at)
               VALUES (sha256('presented'::bytea), 'staff', {ada}, now(), now(), now() + interval '1 hour')"
        ),
    )
    .await;
    assert_eq!(
        join(&pool, BUSINESS, "ada", ada, &[7; 16]).await,
        "redeemed"
    );
    assert_eq!(stored(&database).await, [1, 1, 1, 1, 0, 0]);
    assert_eq!(
        count(
            &database,
            "SELECT count(*) FROM identity.staff_sessions WHERE proven_at IS NULL
               AND token_hash <> sha256('presented'::bytea)"
        )
        .await,
        1,
        "the new session is unproven, and the presented one ended"
    );
    assert_eq!(join(&pool, BUSINESS, "ada", ada, &[8; 16]).await, "joined");
    assert_eq!(stored(&database).await, [1, 1, 1, 1, 0, 0]);
}

/// Joining in a business that does not hold the invite, by a link that names someone else, or with a credential
/// another staff member's passkey has, joins no one and leaves the invite open for its person.
#[tokio::test]
async fn a_link_joins_only_its_person_in_its_business() {
    let (database, pool) = invited().await;
    let (ada, bo) = (staff(&database, "Ada").await, staff(&database, "Bo").await);
    assert_eq!(join(&pool, 4, "ada", ada, &[7; 16]).await, "expired");
    assert_eq!(join(&pool, BUSINESS, "bo", ada, &[7; 16]).await, "expired");
    assert_eq!(
        stored(&database).await,
        [0, 0, 2, 0, 0, 0],
        "nothing joined"
    );
    assert_eq!(
        join(&pool, BUSINESS, "ada", ada, &[7; 16]).await,
        "redeemed"
    );
    assert_eq!(join(&pool, BUSINESS, "bo", bo, &[7; 16]).await, "taken");
    assert_eq!(join(&pool, BUSINESS, "bo", bo, &[9; 15]).await, "oversized");
    as_db_admin(
        &database,
        "UPDATE identity.staff SET disabled_at = now() WHERE name = 'Bo Lim'",
    )
    .await;
    assert_eq!(join(&pool, BUSINESS, "bo", bo, &[9; 16]).await, "disabled");
    assert_eq!(join(&pool, BUSINESS, "bo", 0, &[9; 16]).await, "unknown");
    assert_eq!(
        stored(&database).await,
        [1, 1, 1, 1, 0, 0],
        "Bo's invite is still open"
    );
}

/// Someone with a passkey accepts only their own invite, in its business, from a fresh login: its role is added and
/// the invite used up, once. Anyone else's invite, a stale, unproven, or ended login, no session, an unknown link,
/// and another business are each refused, and nothing is used up.
#[tokio::test]
async fn a_staff_member_accepts_only_their_own_invite() {
    let (database, pool) = invited().await;
    let ada = staff(&database, "Ada").await;
    logged_in(&database, ada).await;
    for (business, link, session, outcome) in [
        (BUSINESS, "bo", "fresh", "other"),
        (BUSINESS, "ada", "stale", "stale"),
        (BUSINESS, "ada", "unproven", "stale"),
        (BUSINESS, "ada", "ended", "logged_out"),
        (BUSINESS, "ada", "nobody's", "logged_out"),
        (BUSINESS, "nobody", "fresh", "expired"),
        (4, "ada", "fresh", "expired"),
    ] {
        assert_eq!(
            accept(&pool, business, link, session).await,
            outcome,
            "{link} with {session} at {business}"
        );
    }
    assert_eq!(
        stored(&database).await,
        [1, 4, 2, 0, 0, 0],
        "nothing is used up"
    );
    assert_eq!(accept(&pool, BUSINESS, "ada", "fresh").await, "accepted");
    assert_eq!(stored(&database).await, [1, 4, 1, 1, 0, 0]);
    assert_eq!(
        accept(&pool, BUSINESS, "ada", "fresh").await,
        "expired",
        "used once"
    );
}

/// Someone who holds a role at the business already accepts a new invite there as a member: the invite is used up
/// and their role is kept. A disabled staff member accepts nothing, and an ended link nothing either.
#[tokio::test]
async fn a_member_s_invite_is_used_up_and_the_disabled_accept_nothing() {
    let (database, pool) = invited().await;
    let ada = staff(&database, "Ada").await;
    logged_in(&database, ada).await;
    as_db_admin(
        &database,
        &format!(
            "INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name) VALUES ({BUSINESS}, {ada}, 'owner');
             UPDATE identity.staff_invites SET ends_at = now() - interval '1 second' WHERE staff_id <> {ada}"
        ),
    )
    .await;
    assert_eq!(accept(&pool, BUSINESS, "bo", "fresh").await, "expired");
    assert_eq!(accept(&pool, BUSINESS, "ada", "fresh").await, "member");
    assert_eq!(stored(&database).await, [1, 4, 1, 1, 0, 0]);
    as_db_admin(&database, "UPDATE identity.staff SET disabled_at = now()").await;
    assert_eq!(accept(&pool, BUSINESS, "ada", "fresh").await, "disabled");
}

/// A join ceremony begins only for a link that is open and names its person, who has no passkey yet, reading the
/// invite through the lookup policy alone.
#[tokio::test]
async fn a_join_ceremony_begins_only_for_an_open_link() {
    let (database, pool) = invited().await;
    let (ada, bo) = (staff(&database, "Ada").await, staff(&database, "Bo").await);
    assert!(begin(&pool, ada, "ada").await);
    assert!(!begin(&pool, ada, "bo").await, "Bo's link is not Ada's");
    logged_in(&database, bo).await;
    assert!(!begin(&pool, bo, "bo").await, "Bo has a passkey already");
    assert_eq!(
        count(
            &database,
            "SELECT count(*) FROM identity.staff_ceremonies WHERE kind = 'join'"
        )
        .await,
        1
    );
}
