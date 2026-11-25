//! Issuing staff invites, in Postgres as the application role (ADR-0089): the staff member an email names is found
//! or made, and the invite is written in the business's own transaction, replacing any open one of theirs there.

use tetsu_identity::{
    Actor, BusinessId, Error, Identity, PersonName, Role, StaffInvite, postgres_staff_store,
};
use tetsu_kernel::Address;
use tetsu_postgres::test_support::{TestDatabase, migrated_database};
use tetsu_postgres::{TenantTransaction, connect, pool};

const FERN: BusinessId = BusinessId::new(3);
const OAK: BusinessId = BusinessId::new(4);

#[cfg(test)]
mod support {
    use super::*;

    pub fn actor() -> Actor {
        Actor::parse("kurt@workstation").expect("an actor")
    }

    pub fn name(text: &str) -> PersonName {
        PersonName::parse(text).expect("a name")
    }

    pub fn email(text: &str) -> Address {
        Address::parse(text).expect("an address")
    }

    pub async fn ready() -> TestDatabase {
        migrated_database(&[&Identity])
            .await
            .expect("a migrated test database")
    }

    /// Rows the database administrator reads, each as text joined by `|`.
    pub async fn rows(database: &TestDatabase, sql: &str) -> Vec<String> {
        connect(database)
            .await
            .expect("connects")
            .query(
                &format!("SELECT concat_ws('|', VARIADIC r) FROM ({sql}) AS q (r)"),
                &[],
            )
            .await
            .expect("reads")
            .iter()
            .map(|row| row.try_get(0).expect("text"))
            .collect()
    }

    pub async fn as_db_admin(database: &TestDatabase, sql: &str) {
        connect(database)
            .await
            .expect("connects")
            .batch_execute(sql)
            .await
            .expect("runs");
    }

    /// Issues an invite as the staff command does.
    pub async fn invite(
        database: &TestDatabase,
        business: BusinessId,
        who: &str,
        at: &str,
    ) -> Result<StaffInvite, Error> {
        postgres_staff_store(pool(&database.as_app()).expect("a pool"))
            .invite_staff(business, &name(who), &email(at), Role::Owner, &actor())
            .await
    }

    /// Who an invite link names, as the join page reads it: its business and staff member.
    pub async fn invitee(database: &TestDatabase, invite: &StaffInvite) -> Vec<String> {
        rows(
            database,
            &format!(
                "SELECT ARRAY[tenant_id::text, staff_id::text, name, role_name, joined::text]
                 FROM identity.staff_invitee(sha256(decode('{}', 'hex')))",
                invite.token.to_text()
            ),
        )
        .await
    }

    /// The hash of a token as Identity stores it, written as hexadecimal: the SHA-256 of its bytes.
    pub async fn stored(database: &TestDatabase, invite: &StaffInvite) -> String {
        rows(
            database,
            &format!(
                "SELECT ARRAY[encode(sha256(decode('{}', 'hex')), 'hex')]",
                invite.token.to_text()
            ),
        )
        .await
        .remove(0)
    }

    /// Runs `sql` as Identity's role in a business's own transaction, left open, then issues an invite through the use
    /// case, and commits only once the invite waits on a lock the open transaction holds, so the two overlap;
    /// returns what the invite came to.
    pub async fn issued_meanwhile(
        database: &TestDatabase,
        business: BusinessId,
        sql: &str,
        then: (BusinessId, &str, &str),
    ) -> Result<StaffInvite, Error> {
        let held = pool(&database.as_app()).expect("a pool");
        let mut client = held.get().await.expect("a connection");
        let open = TenantTransaction::begin(&mut client, "identity", business.get())
            .await
            .expect("begins");
        open.batch_execute(sql).await.expect("runs");
        let committing = async {
            waits_for_a_lock(database).await;
            open.commit().await.expect("commits");
        };
        let (issued, ()) = tokio::join!(invite(database, then.0, then.1, then.2), committing);
        issued
    }

    /// Returns once some statement in the database waits on a lock, as an invite does on a transaction left open.
    pub async fn waits_for_a_lock(database: &TestDatabase) {
        let client = connect(database).await.expect("connects");
        for _ in 0..500 {
            let waiting: i64 = client
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'",
                    &[],
                )
                .await
                .expect("reads")
                .try_get(0)
                .expect("a count");
            if waiting > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("nothing waited on a lock");
    }

    /// The SQL an invite to Ada at this business runs, under a link `link`'s hash.
    pub fn issuing(link: &str) -> String {
        format!(
            "SELECT identity.staff_issue_invite(sha256('{link}'::bytea), 'Ada Park', 'ada@example.com', 'owner')"
        )
    }
}

use support::{as_db_admin, invite, invitee, issued_meanwhile, issuing, ready, rows, stored};

#[tokio::test]
async fn a_new_email_makes_a_staff_member_and_invites_them_for_seven_days() {
    let database = ready().await;
    let issued = invite(&database, FERN, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    assert!(issued.created);
    assert!(!issued.renamed);
    let id = issued.staff.get();
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[id::text, name, email] FROM identity.staff"
        )
        .await,
        [format!("{id}|Ada Park|ada@example.com")]
    );
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[tenant_id::text, encode(token_hash, 'hex'), role_name,
               (ends_at - issued_at = interval '7 days')::text]
             FROM identity.staff_invites"
        )
        .await,
        [format!("3|{}|owner|true", stored(&database, &issued).await)]
    );
    assert_eq!(
        invitee(&database, &issued).await,
        [format!("3|{id}|Ada Park|owner|false")]
    );
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[event_type, payload::text] FROM identity.outbox_global"
        )
        .await,
        [format!(
            r#"identity.staff_invited|{{"role": "owner", "actor": "kurt@workstation", "reason": null, "created": true, "outcome": "changed", "staff_id": {id}, "business_id": 3}}"#
        )]
    );
    // The business's own record learns nothing of the operator's invite, such as whether the person is staff
    // elsewhere.
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[count(*)::text] FROM identity.outbox"
        )
        .await,
        ["0"]
    );
}

#[tokio::test]
async fn an_email_on_file_keeps_its_staff_member_and_their_name() {
    let database = ready().await;
    let first = invite(&database, FERN, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    let second = invite(&database, OAK, "Ada P", "ADA@example.com")
        .await
        .expect("issues");
    assert_eq!(second.staff, first.staff);
    assert!(!second.created);
    assert!(second.renamed);
    let same = invite(&database, OAK, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    assert!(!same.renamed);
    assert_eq!(
        rows(&database, "SELECT ARRAY[name, email] FROM identity.staff").await,
        ["Ada Park|ada@example.com"]
    );
    // One invite at each business, each its own link.
    assert_eq!(invitee(&database, &first).await.len(), 1);
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[tenant_id::text] FROM identity.staff_invites ORDER BY tenant_id"
        )
        .await,
        ["3", "4"]
    );
}

#[tokio::test]
async fn a_new_invite_replaces_the_open_one_at_its_business() {
    let database = ready().await;
    let old = invite(&database, FERN, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    let new = invite(&database, FERN, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    assert_eq!(invitee(&database, &old).await, Vec::<String>::new());
    assert_eq!(invitee(&database, &new).await.len(), 1);
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[count(*)::text] FROM identity.staff_invites"
        )
        .await,
        ["1"]
    );
}

#[tokio::test]
async fn a_member_or_a_disabled_staff_member_is_issued_nothing() {
    let database = ready().await;
    let ada = invite(&database, FERN, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    let bo = invite(&database, FERN, "Bo Lim", "bo@example.com")
        .await
        .expect("issues");
    as_db_admin(
        &database,
        &format!(
            "INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name) VALUES (3, {}, 'owner');
             UPDATE identity.staff SET disabled_at = now() WHERE id = {}",
            ada.staff.get(),
            bo.staff.get()
        ),
    )
    .await;
    let member = invite(&database, FERN, "Ada Park", "ada@example.com").await;
    assert!(
        matches!(&member, Err(Error::Invalid(why)) if why == "the staff member at ada@example.com holds a role at this business already"),
        "{member:?}"
    );
    let disabled = invite(&database, OAK, "Bo Lim", "bo@example.com").await;
    assert!(
        matches!(&disabled, Err(Error::Invalid(why)) if why == "the staff member at bo@example.com is disabled"),
        "{disabled:?}"
    );
    // The open invites stand as they were, and each refusal is recorded in Tetsu's record, naming its business.
    assert_eq!(invitee(&database, &ada).await.len(), 1);
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[payload->>'business_id', payload->>'outcome', payload->>'reason']
             FROM identity.outbox_global WHERE payload->>'outcome' = 'refused'
             ORDER BY payload->>'business_id'"
        )
        .await,
        ["3|refused|member", "4|refused|disabled"]
    );
    // Another business may still invite Ada, who holds no role there.
    invite(&database, OAK, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
}

/// An invite that meets another to the same new email, at another business, waits for it, and finds the staff member
/// it made, so there is one.
#[tokio::test]
async fn invites_at_once_to_one_new_email_make_one_staff_member() {
    let database = ready().await;
    let oak = issued_meanwhile(
        &database,
        FERN,
        &issuing("fern"),
        (OAK, "Ada Park", "Ada@Example.com"),
    )
    .await
    .expect("issues");
    assert!(!oak.created);
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[count(*)::text] FROM identity.staff"
        )
        .await,
        ["1"]
    );
}

/// Two invites at once to the same person at one business leave one open invite, the later one's, and neither fails,
/// whether the email was new or the person held an open invite there.
#[tokio::test]
async fn invites_at_once_at_one_business_leave_the_later_one() {
    let database = ready().await;
    let new = issued_meanwhile(
        &database,
        FERN,
        &issuing("first"),
        (FERN, "Ada Park", "ada@example.com"),
    )
    .await
    .expect("issues");
    invite(&database, OAK, "Bo Lim", "bo@example.com")
        .await
        .expect("issues");
    let on_file = issued_meanwhile(
        &database,
        OAK,
        &issuing("x").replace("Ada Park', 'ada@", "Bo Lim', 'bo@"),
        (OAK, "Bo Lim", "bo@example.com"),
    )
    .await
    .expect("issues");
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[tenant_id::text, encode(token_hash, 'hex')] FROM identity.staff_invites ORDER BY tenant_id"
        )
        .await,
        [
            format!("3|{}", stored(&database, &new).await),
            format!("4|{}", stored(&database, &on_file).await)
        ]
    );
}

/// Two invites at once to someone on file with no open invite at the business, so neither finds one to wait on, leave
/// one open invite, the later one's, and neither fails.
#[tokio::test]
async fn invites_at_once_that_find_no_open_invite_leave_the_later_one() {
    let database = ready().await;
    invite(&database, OAK, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    let later = issued_meanwhile(
        &database,
        FERN,
        &issuing("first"),
        (FERN, "Ada Park", "ada@example.com"),
    )
    .await
    .expect("issues");
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[encode(token_hash, 'hex')] FROM identity.staff_invites WHERE tenant_id = 3"
        )
        .await,
        [stored(&database, &later).await]
    );
}

/// An invite issued while the person's join at the business is under way waits for it, then finds the role it gave,
/// so it is refused and writes no invite.
#[tokio::test]
async fn an_invite_during_a_join_finds_the_role_it_gives() {
    let database = ready().await;
    let first = invite(&database, FERN, "Ada Park", "ada@example.com")
        .await
        .expect("issues");
    let joining = format!(
        "SELECT identity.staff_spend_invite(sha256(decode('{}', 'hex')), {})",
        first.token.to_text(),
        first.staff.get()
    );
    let during = issued_meanwhile(
        &database,
        FERN,
        &joining,
        (FERN, "Ada Park", "ada@example.com"),
    )
    .await;
    assert!(
        matches!(&during, Err(Error::Invalid(why)) if why.ends_with("holds a role at this business already")),
        "{during:?}"
    );
    assert_eq!(
        rows(
            &database,
            "SELECT ARRAY[count(*)::text] FROM identity.staff_invites"
        )
        .await,
        ["0"]
    );
}
