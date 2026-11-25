//! Identity's Postgres adapter. Driver types stay in this file (ADR-0007).

use std::fmt::Write;
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{Value, json};
use tetsu_kernel::{Address, Migration, Requester};
use tetsu_postgres::{Failure, ModuleTransaction, NewEvent, Pool};
use tokio_postgres::Row;
use tokio_postgres::types::{FromSql, ToSql};

use crate::domain::{
    Account, AccountId, Actor, Addition, AdminName, AdminSummary, CeremonyKind, Disabling, Ending,
    Error, FailedCheck, HeldPasskey, Invitee, Issue, ListedSession, Login, NewPasskey, RateLimit,
    Realm, Redemption, Refusal, Removal, Session, SessionId, SessionStart, Signer, TakenAddition,
    TakenCeremony, TimeLeft,
};
use crate::ports::{AdminDirectory, AdminStore, LoginStore};
use crate::use_cases::Accounts;

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../migrations/0001_admins.sql"),
    },
    // Identity records what the admin command does, for the audit record (ADR-0043, ADR-0045).
    Migration {
        version: 2,
        sql: tetsu_postgres::OUTBOX,
    },
    Migration {
        version: 3,
        sql: include_str!("../../migrations/0003_session_left.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("../../migrations/0004_rate_limits.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("../../migrations/0005_admin_email.sql"),
    },
    Migration {
        version: 6,
        sql: tetsu_postgres::INBOX,
    },
    Migration {
        version: 7,
        sql: include_str!("../../migrations/0007_set_admin_email.sql"),
    },
    Migration {
        version: 8,
        sql: include_str!("../../migrations/0008_session_list.sql"),
    },
    Migration {
        version: 9,
        sql: tetsu_postgres::INBOX_DELIVERY,
    },
    Migration {
        version: 10,
        sql: include_str!("../../migrations/0010_refusal_limit.sql"),
    },
    Migration {
        version: 11,
        sql: include_str!("../../migrations/0011_validate_refusal_limit.sql"),
    },
    Migration {
        version: 12,
        sql: include_str!("../../migrations/0012_log_out.sql"),
    },
    Migration {
        version: 13,
        sql: include_str!("../../migrations/0013_staff.sql"),
    },
];
/// Identity's schema, which names the role its transactions act as (ADR-0042).
const SCHEMA: &str = "identity";

const INSERT_ADMIN: &str = include_str!("../../sql/insert_admin.sql");
const ADMIN_EMAIL: &str = include_str!("../../sql/admin_email.sql");
const SET_ADMIN_EMAIL: &str = include_str!("../../sql/set_admin_email.sql");
const USE_SESSION: &str = include_str!("../../sql/use_session.sql");
const SESSION_LEFT: &str = include_str!("../../sql/session_left.sql");
const LOG_OUT: &str = include_str!("../../sql/log_out.sql");
const ISSUE_INVITE: &str = include_str!("../../sql/issue_invite.sql");
const DISABLE_ADMIN: &str = include_str!("../../sql/disable_admin.sql");
const END_ADMIN_SESSIONS: &str = include_str!("../../sql/end_admin_sessions.sql");
const END_REALM_SESSIONS: &str = include_str!("../../sql/end_realm_sessions.sql");
const LIST_ADMINS: &str = include_str!("../../sql/list_admins.sql");
const ENABLE_ADMIN: &str = include_str!("../../sql/enable_admin.sql");
const INVITEE: &str = include_str!("../../sql/invitee.sql");
const REMOVE_PASSKEYS: &str = include_str!("../../sql/remove_passkeys.sql");
const BEGIN_LOGIN: &str = include_str!("../../sql/begin_login.sql");
const ADMIT: &str = include_str!("../../sql/admit.sql");
const FAIL: &str = include_str!("../../sql/fail.sql");
const TAKE_LOGIN: &str = include_str!("../../sql/take_login.sql");
const SIGNER: &str = include_str!("../../sql/signer.sql");
const LOGIN: &str = include_str!("../../sql/login.sql");
const ACCOUNT: &str = include_str!("../../sql/account.sql");
const ADDER: &str = include_str!("../../sql/adder.sql");
const BEGIN_ADDITION: &str = include_str!("../../sql/begin_addition.sql");
const TAKE_ADDITION: &str = include_str!("../../sql/take_addition.sql");
const ADD_PASSKEY: &str = include_str!("../../sql/add_passkey.sql");
const RECORD: &str = include_str!("../../sql/record.sql");
const BEGIN_JOIN: &str = include_str!("../../sql/begin_join.sql");
const TAKE_CEREMONY: &str = include_str!("../../sql/take_ceremony.sql");
const REDEEM_INVITE: &str = include_str!("../../sql/redeem_invite.sql");
const SESSIONS_OF: &str = include_str!("../../sql/sessions_of.sql");
const END_OTHER_SESSIONS: &str = include_str!("../../sql/end_other_sessions.sql");
/// The kind of ceremony join keeps.
const JOIN: &str = "join";

/// An event for the audit record: its type, and a payload of IDs and facts, never names or tokens (ADR-0045).
type Recorded = Option<(&'static str, String)>;
/// What a statement's rows came to, if anything, and the event that records it.
type Decided<T> = Result<(Option<T>, Recorded), Error>;
/// What makes a statement's event of the rows it returned.
type MakesEvent<'a> = &'a (dyn Fn(&[Row]) -> Result<Recorded, Error> + Sync);

/// Identity's admin realm over Postgres, for the composition root. It returns the use cases, never the store
/// itself (ADR-0021).
///
/// The use cases are there:
///
/// ```no_run
/// # async fn reach(pool: tetsu_postgres::Pool) {
/// let _ = tetsu_identity::postgres_store(pool).session(["x"]).await;
/// # }
/// ```
///
/// The handle is not a store of any port, whatever their methods are called:
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn store(_: &impl tetsu_identity::AdminStore) {}
/// store(&tetsu_identity::postgres_store(pool));
/// # }
/// ```
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn store(_: &impl tetsu_identity::LoginStore) {}
/// store(&tetsu_identity::postgres_store(pool));
/// # }
/// ```
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn store(_: &impl tetsu_identity::AdminDirectory) {}
/// store(&tetsu_identity::postgres_store(pool));
/// # }
/// ```
///
/// It derefs to nothing, so no store's methods are reached through it:
///
/// ```compile_fail,E0277
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// fn derefs(_: &impl std::ops::Deref) {}
/// derefs(&tetsu_identity::postgres_store(pool));
/// # }
/// ```
///
/// And the store inside it is private:
///
/// ```compile_fail,E0616
/// # fn bypass(pool: tetsu_postgres::Pool) {
/// let _ = &tetsu_identity::postgres_store(pool).store;
/// # }
/// ```
#[must_use]
pub fn postgres_store(pool: Pool) -> Accounts<impl AdminStore> {
    Accounts::new(PostgresAdmins { pool })
}

pub(crate) struct PostgresAdmins {
    pub(crate) pool: Pool,
}

/// Every statement runs alone in a transaction as Identity's role, which reaches sessions only through its lookup
/// functions (ADR-0042).
impl PostgresAdmins {
    async fn query(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error> {
        self.query_recording(statement, parameters, &|_| Ok(None))
            .await
    }

    /// Runs a lookup function that answers yes, no, or nothing in its `done` column, and records the event `event`
    /// makes of the answer, whatever it is, in the same transaction.
    async fn change(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
        event: impl Fn(Option<bool>) -> (&'static str, String) + Sync,
    ) -> Result<Option<bool>, Error> {
        self.decided(statement, parameters, |rows| {
            let answer = answered(rows, "done")?;
            Ok((answer, Some(event(answer))))
        })
        .await
    }

    /// Runs a lookup function that ends sessions and counts them, or answers nothing for an unknown admin, and
    /// records the event `event` makes of the count.
    async fn ended(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
        event: impl Fn(Option<u64>) -> (&'static str, String) + Sync,
    ) -> Result<Option<u64>, Error> {
        self.decided(statement, parameters, |rows| {
            let count = counted(rows, "ended")?;
            Ok((count, Some(event(count))))
        })
        .await
    }

    /// Runs a statement whose answer `read` makes of the rows it returned, with the event it records, in the same
    /// transaction, and returns the answer: none for an admin or passkey that does not exist.
    async fn decided<T: Copy + Send + Sync>(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
        read: impl Fn(&[Row]) -> Decided<T> + Sync,
    ) -> Result<Option<T>, Error> {
        let answer = OnceLock::new();
        self.query_recording(statement, parameters, &|rows| {
            let (decided, recorded) = read(rows)?;
            let _ = answer.set(decided);
            Ok(recorded)
        })
        .await?;
        Ok(answer.get().copied().flatten())
    }

    /// Runs a statement, and records the event `event` makes of what it returned, if any, in the same transaction,
    /// so a change is never left unrecorded, nor an event recorded for a change that rolled back. One build of this
    /// serves every caller, so every path through it is tested once.
    async fn query_recording(
        &self,
        statement: &str,
        parameters: &[&(dyn ToSql + Sync)],
        event: MakesEvent<'_>,
    ) -> Result<Vec<Row>, Error> {
        let mut client = self.pool.get().await.map_err(pool_failed)?;
        let result = run(&mut client, statement, parameters, event).await;
        if let Err(Error::Unavailable(why)) = &result {
            tetsu_postgres::clear_if_stale(&self.pool, why);
        }
        result
    }
}

async fn run(
    client: &mut tetsu_postgres::Object,
    statement: &str,
    parameters: &[&(dyn ToSql + Sync)],
    event: MakesEvent<'_>,
) -> Result<Vec<Row>, Error> {
    let transaction = ModuleTransaction::begin(client, SCHEMA)
        .await
        .map_err(|e| sorted(&e))?;
    let prepared = transaction
        .prepare_cached(statement)
        .await
        .map_err(|e| sorted(&e))?;
    let rows = transaction
        .query(&prepared, parameters)
        .await
        .map_err(|e| sorted(&e))?;
    if let Some((kind, payload)) = event(&rows)? {
        let event = NewEvent {
            kind,
            version: 1,
            payload: &payload,
            keep: false,
        };
        transaction.record(&event).await.map_err(|e| sorted(&e))?;
    }
    transaction.commit().await.map_err(|e| commit_failed(&e))?;
    Ok(rows)
}

/// How a statement failed. Identity's own functions raise three codes of their own, and each is a fault of the code
/// that called them, so storage: TD001, a session started for an admin not enabled, which only a login that
/// holds an enabled admin's row ever starts; TD002, an isolation level other than read committed; and TD003, a
/// login that would change more of a passkey than its counter and backup flags.
fn sorted(error: &tokio_postgres::Error) -> Error {
    failed(tetsu_postgres::sort(error))
}

fn commit_failed(error: &tokio_postgres::Error) -> Error {
    failed(tetsu_postgres::sort_commit(error))
}

fn pool_failed(error: deadpool_postgres::PoolError) -> Error {
    failed(tetsu_postgres::sort_pool(error))
}

/// Identity holds no row a caller may take by its value, so a taken row is a storage failure like any other.
fn failed(failure: Failure) -> Error {
    match failure {
        Failure::Temporary(why) => Error::Unavailable(why),
        Failure::Taken { message, .. } | Failure::Permanent(message) => Error::Storage(message),
    }
}

fn column<'a, T: FromSql<'a>>(row: &'a Row, name: &str) -> Result<T, Error> {
    row.try_get(name).map_err(|e| sorted(&e))
}

impl LoginStore for PostgresAdmins {
    const REALM: Realm = Realm::Admin;

    async fn use_session(&self, hash: &[u8; 32], realm: Realm) -> Result<Option<Session>, Error> {
        let rows = self
            .query(USE_SESSION, &[&hash.as_slice(), &realm.as_str()])
            .await?;
        rows.first().map(|row| session_from(row, hash)).transpose()
    }

    async fn session_left(&self, hash: &[u8; 32], realm: Realm) -> Result<Option<TimeLeft>, Error> {
        let rows = self
            .query(SESSION_LEFT, &[&hash.as_slice(), &realm.as_str()])
            .await?;
        rows.first()
            .map(|row| {
                Ok(TimeLeft {
                    idle: column(row, "idle_left")?,
                    absolute: column(row, "absolute_left")?,
                })
            })
            .transpose()
    }

    async fn delete_session(
        &self,
        hash: &[u8; 32],
        realm: Realm,
        actor: &Actor,
    ) -> Result<bool, Error> {
        let parameters: [&(dyn ToSql + Sync); 2] = [&hash.as_slice(), &realm.as_str()];
        let ended = self
            .decided(LOG_OUT, &parameters, |rows| logged_out(rows, realm, actor))
            .await?;
        Ok(ended.unwrap_or(false))
    }

    async fn invitee(&self, link: &[u8; 32]) -> Result<Option<Invitee>, Error> {
        self.query(INVITEE, &[&link.as_slice()])
            .await?
            .first()
            .map(invitee_from)
            .transpose()
    }

    async fn begin_join(
        &self,
        token: &[u8; 32],
        admin: AccountId,
        link: &[u8; 32],
        state: &str,
    ) -> Result<bool, Error> {
        let parameters: [&(dyn ToSql + Sync); 4] =
            [&token.as_slice(), &admin.get(), &link.as_slice(), &state];
        begun(&self.query(BEGIN_JOIN, &parameters).await?)
    }

    async fn take_join(&self, token: &[u8; 32]) -> Result<Option<TakenCeremony>, Error> {
        self.query(TAKE_CEREMONY, &[&token.as_slice(), &JOIN])
            .await?
            .first()
            .map(ceremony_from)
            .transpose()
    }

    async fn redeem_invite(
        &self,
        ceremony: &TakenCeremony,
        passkey: &NewPasskey,
        session: &SessionStart,
        actor: &Actor,
    ) -> Result<Option<Redemption>, Error> {
        let id = ceremony.admin.get();
        let transports = passkey.transports.join(" ");
        let replacing = session.replacing.as_ref().map(<[u8; 32]>::as_slice);
        let device = actor.device().map(|device| device.to_string());
        let parameters: [&(dyn ToSql + Sync); 8] = [
            &ceremony.link_hash.as_slice(),
            &id,
            &passkey.credential_id,
            &passkey.passkey,
            &transports,
            &session.hash.as_slice(),
            &replacing,
            &device,
        ];
        self.decided(REDEEM_INVITE, &parameters, |rows| {
            redemption(rows, id, &passkey.credential_id, actor)
        })
        .await
    }

    async fn admit(&self, limit: RateLimit, from: &Requester) -> Result<Option<Duration>, Error> {
        let rows = self
            .query(ADMIT, &[&limit.as_str(), &from.as_bytes().as_slice()])
            .await?;
        rows.first()
            .map(|row| column::<Option<i64>>(row, "wait"))
            .transpose()
            .map(|wait| {
                wait.flatten()
                    .map(|seconds| Duration::from_secs(seconds.unsigned_abs()))
            })
    }

    async fn record_refusal(
        &self,
        from: &Requester,
        route: &'static str,
        check: FailedCheck,
        actor: &Actor,
    ) -> Result<(), Error> {
        let network = from.as_bytes().as_slice();
        let payload = json!({"route": route, "reason": check.as_str()});
        // Recorded while the network has room; past that, the first refusal of each window says that the ones
        // after it go unrecorded, so a stretch with no records is never silent.
        for (limit, kind) in [
            (RateLimit::Refusal, "identity.request_refused"),
            (RateLimit::RefusalFlood, "identity.refusals_suppressed"),
        ] {
            let admitted = self
                .decided(ADMIT, &[&limit.as_str(), &network], |rows| {
                    admitted_to_record(rows, kind, actor, &payload)
                })
                .await?;
            if admitted == Some(true) {
                break;
            }
        }
        Ok(())
    }

    async fn fail(&self, from: &Requester) -> Result<(), Error> {
        let failure = RateLimit::LoginFailure.as_str();
        self.query(FAIL, &[&failure, &from.as_bytes().as_slice()])
            .await
            .map(drop)
    }

    async fn begin_login(&self, token: &[u8; 32], state: &str) -> Result<(), Error> {
        self.query(BEGIN_LOGIN, &[&token.as_slice(), &state])
            .await
            .map(drop)
    }

    async fn refuse(&self, refusal: &Refusal<'_>, actor: &Actor) -> Result<(), Error> {
        let payload = json!({
            "admin_id": refusal.admin.map(AccountId::get),
            "credential": refusal.credential.map(hex),
        });
        self.query_recording(RECORD, &[], &|_| {
            Ok(Some(reasoned(
                event_kind(refusal.kind),
                actor,
                Some(refusal.reason),
                payload.clone(),
            )))
        })
        .await
        .map(drop)
    }

    async fn take_login(&self, token: &[u8; 32]) -> Result<Option<String>, Error> {
        taken_state(&self.query(TAKE_LOGIN, &[&token.as_slice()]).await?)
    }

    async fn signer(&self, credential: &[u8]) -> Result<Option<Signer>, Error> {
        self.query(SIGNER, &[&credential])
            .await?
            .first()
            .map(|row| signer_from(row, credential))
            .transpose()
    }

    async fn login(
        &self,
        signer: &Signer,
        passkey: &str,
        session: &SessionStart,
        actor: &Actor,
    ) -> Result<Option<Login>, Error> {
        let id = signer.admin.get();
        let credential = signer.credential.as_slice();
        let replacing = session.replacing.as_ref().map(<[u8; 32]>::as_slice);
        let device = actor.device().map(|device| device.to_string());
        let parameters: [&(dyn ToSql + Sync); 7] = [
            &credential,
            &id,
            &signer.passkey,
            &passkey,
            &session.hash.as_slice(),
            &replacing,
            &device,
        ];
        self.decided(LOGIN, &parameters, |rows| {
            logging_in(rows, signer, passkey, actor)
        })
        .await
    }

    async fn account(&self, session: &[u8; 32]) -> Result<Option<Account>, Error> {
        account_from(&self.query(ACCOUNT, &[&session.as_slice()]).await?)
    }

    async fn adder(&self, session: &[u8; 32]) -> Result<Option<Account>, Error> {
        account_from(&self.query(ADDER, &[&session.as_slice()]).await?)
    }

    async fn sessions_of(&self, session: &[u8; 32]) -> Result<Vec<ListedSession>, Error> {
        self.query(SESSIONS_OF, &[&session.as_slice()])
            .await?
            .iter()
            .map(listed)
            .collect()
    }

    async fn end_other_sessions(
        &self,
        session: &[u8; 32],
        only: Option<&SessionId>,
        actor: &Actor,
    ) -> Result<Ending, Error> {
        let only = only.map(SessionId::as_str);
        let parameters: [&(dyn ToSql + Sync); 2] = [&session.as_slice(), &only];
        let ending = self
            .decided(END_OTHER_SESSIONS, &parameters, |rows| {
                ending(rows, only, actor)
            })
            .await?;
        Ok(ending.unwrap_or(Ending::NoSession))
    }

    async fn begin_addition(
        &self,
        token: &[u8; 32],
        session: &[u8; 32],
        admin: AccountId,
        state: &str,
    ) -> Result<bool, Error> {
        let parameters: [&(dyn ToSql + Sync); 4] =
            [&token.as_slice(), &session.as_slice(), &admin.get(), &state];
        begun(&self.query(BEGIN_ADDITION, &parameters).await?)
    }

    async fn take_addition(
        &self,
        token: &[u8; 32],
        session: &[u8; 32],
    ) -> Result<Option<TakenAddition>, Error> {
        self.query(TAKE_ADDITION, &[&token.as_slice(), &session.as_slice()])
            .await?
            .first()
            .map(|row| taken_addition(row, token, session))
            .transpose()
    }

    async fn add_passkey(
        &self,
        addition: &TakenAddition,
        passkey: &NewPasskey,
        actor: &Actor,
    ) -> Result<Option<Addition>, Error> {
        let transports = passkey.transports.join(" ");
        let parameters: [&(dyn ToSql + Sync); 6] = [
            &addition.token.as_slice(),
            &addition.session.as_slice(),
            &addition.admin.get(),
            &passkey.credential_id,
            &passkey.passkey,
            &transports,
        ];
        self.decided(ADD_PASSKEY, &parameters, |rows| {
            adding(rows, addition.admin.get(), passkey, actor)
        })
        .await
    }
}

impl AdminDirectory for PostgresAdmins {
    async fn insert_admin(
        &self,
        name: &AdminName,
        email: Option<&Address>,
        actor: &Actor,
    ) -> Result<AccountId, Error> {
        let parameters: [&(dyn ToSql + Sync); 2] = [&name.as_str(), &email.map(Address::as_str)];
        created(
            self.decided(INSERT_ADMIN, &parameters, |rows| creation(rows, actor))
                .await?,
        )
    }

    async fn admin_email(&self, admin: AccountId) -> Result<Option<Address>, Error> {
        self.query(ADMIN_EMAIL, &[&admin.get()])
            .await?
            .first()
            .map_or(Ok(None), |row| column::<Option<String>>(row, "email"))
            .and_then(|email| email.as_deref().map(address_from).transpose())
    }

    async fn set_admin_email(
        &self,
        admin: AccountId,
        email: &Address,
        actor: &Actor,
    ) -> Result<Option<bool>, Error> {
        let id = admin.get();
        // The event names the admin, never the address, as creating one does.
        self.change(SET_ADMIN_EMAIL, &[&id, &email.as_str()], |changed| {
            event(
                "identity.admin_email_set",
                actor,
                outcome(changed),
                json!({"admin_id": id}),
            )
        })
        .await
    }

    async fn issue_invite(
        &self,
        hash: &[u8; 32],
        admin: AccountId,
        actor: &Actor,
    ) -> Result<Option<Issue>, Error> {
        let id = admin.get();
        let parameters: [&(dyn ToSql + Sync); 2] = [&hash.as_slice(), &id];
        self.decided(ISSUE_INVITE, &parameters, |rows| issuance(rows, id, actor))
            .await
    }

    async fn remove_passkeys(
        &self,
        admin: AccountId,
        actor: &Actor,
    ) -> Result<Option<Removal>, Error> {
        let id = admin.get();
        self.decided(REMOVE_PASSKEYS, &[&id], |rows| removing(rows, id, actor))
            .await
    }

    async fn disable_admin(
        &self,
        admin: AccountId,
        actor: &Actor,
    ) -> Result<Option<Disabling>, Error> {
        let id = admin.get();
        self.decided(DISABLE_ADMIN, &[&id], |rows| {
            let found = rows.first().map(disabling_from).transpose()?.flatten();
            let payload = found.map_or_else(
                || json!({"admin_id": id}),
                |done| {
                    json!({
                        "admin_id": id,
                        "sessions_ended": done.sessions_ended,
                        "links_ended": done.links_ended,
                    })
                },
            );
            let outcome = outcome(found.map(|done| done.done));
            Ok((
                found,
                Some(event("identity.admin_disabled", actor, outcome, payload)),
            ))
        })
        .await
    }

    async fn enable_admin(&self, admin: AccountId, actor: &Actor) -> Result<Option<bool>, Error> {
        let id = admin.get();
        self.change(ENABLE_ADMIN, &[&id], |enabled| {
            let payload = json!({"admin_id": id});
            event("identity.admin_enabled", actor, outcome(enabled), payload)
        })
        .await
    }

    async fn end_admin_sessions(
        &self,
        admin: AccountId,
        realm: Realm,
        actor: &Actor,
    ) -> Result<Option<u64>, Error> {
        let (id, name) = (admin.get(), realm.as_str());
        let parameters: [&(dyn ToSql + Sync); 2] = [&id, &name];
        self.ended(END_ADMIN_SESSIONS, &parameters, |ended| {
            let payload = json!({"admin_id": id, "realm": name, "ended": ended});
            event(
                "identity.sessions_ended",
                actor,
                outcome(ended.map(|ended| ended > 0)),
                payload,
            )
        })
        .await
    }

    async fn end_realm_sessions(&self, realm: Realm, actor: &Actor) -> Result<u64, Error> {
        let name = realm.as_str();
        let ended = self
            .ended(END_REALM_SESSIONS, &[&name], |ended| {
                let payload = json!({"realm": name, "ended": ended});
                event(
                    "identity.sessions_ended",
                    actor,
                    outcome(ended.map(|ended| ended > 0)),
                    payload,
                )
            })
            .await?;
        ended.ok_or_else(answered_nothing)
    }

    async fn list_admins(&self) -> Result<Vec<AdminSummary>, Error> {
        self.query(LIST_ADMINS, &[])
            .await?
            .iter()
            .map(summary_from)
            .collect()
    }
}

/// An audit event: its type, and a payload of IDs, counts, and facts, never names or tokens, with who ran the
/// command and what came of it: `changed`, `unchanged`, or `refused` (ADR-0045, `v5.0.0-16.2.1`).
fn event(
    kind: &'static str,
    actor: &Actor,
    outcome: &str,
    mut payload: Value,
) -> (&'static str, String) {
    payload["actor"] = json!(actor.as_str());
    payload["outcome"] = json!(outcome);
    (kind, payload.to_string())
}

/// An event whose reason decides its outcome: changed with none, and refused with one, which the payload names.
fn reasoned(
    kind: &'static str,
    actor: &Actor,
    reason: Option<&str>,
    mut payload: Value,
) -> (&'static str, String) {
    payload["reason"] = json!(reason);
    event(
        kind,
        actor,
        if reason.is_none() {
            "changed"
        } else {
            "refused"
        },
        payload,
    )
}

/// The yes, no, or nothing a lookup function answers in a column; no row at all is nothing too.
fn answered(rows: &[Row], name: &str) -> Result<Option<bool>, Error> {
    rows.first().map_or(Ok(None), |row| column(row, name))
}

/// The count a lookup function answers, or nothing for an admin who does not exist.
fn counted(rows: &[Row], name: &str) -> Result<Option<u64>, Error> {
    let ended: Option<i64> = rows.first().map_or(Ok(None), |row| column(row, name))?;
    ended.map(nonnegative).transpose()
}

/// A count read from a column, which must be a whole number that is not negative.
fn count(row: &Row, name: &str) -> Result<u64, Error> {
    column(row, name).and_then(nonnegative)
}

/// A count Identity's database answers, which it never writes negative: a negative one is a fault of storage, never
/// read as none.
fn nonnegative(count: i64) -> Result<u64, Error> {
    u64::try_from(count).map_err(|_| Error::Storage("a count is negative".to_string()))
}

/// Reads what a create found, and makes the event it records: changed when it added the
/// admin, and refused when the name was taken, naming
/// who holds it if the statement could see them.
fn creation(rows: &[Row], actor: &Actor) -> Decided<(i64, bool)> {
    let found = rows.first().map(added_from).transpose()?;
    Ok((found, Some(creation_event(found, actor))))
}

/// The event a create records for what it found.
fn creation_event(found: Option<(i64, bool)>, actor: &Actor) -> (&'static str, String) {
    let outcome = if matches!(found, Some((_, true))) {
        "changed"
    } else {
        "refused"
    };
    event(
        "identity.admin_created",
        actor,
        outcome,
        json!({"admin_id": found.map(|(id, _)| id)}),
    )
}

/// What a create returns: the new admin, or why there is none.
fn created(found: Option<(i64, bool)>) -> Result<AccountId, Error> {
    match found {
        Some((id, true)) => Ok(AccountId::new(id)),
        Some((id, false)) => Err(Error::Invalid(format!("admin {id} already has that name"))),
        None => Err(Error::Invalid(
            "another admin already has that name".to_string(),
        )),
    }
}

/// Reads what issuing a link came to, and makes the event it records: changed when a link
/// was issued, and refused otherwise, with the reason.
fn issuance(rows: &[Row], id: i64, actor: &Actor) -> Decided<Issue> {
    let issue = rows
        .first()
        .map_or(Ok(None), |row| column::<Option<String>>(row, "issued"))
        .and_then(|answer| answer.as_deref().map(issue_from).transpose())?;
    Ok((issue, Some(issuance_event(issue, id, actor))))
}

/// A stored address that is not plain is a storage failure, since only a plain one is ever written.
fn address_from(text: &str) -> Result<Address, Error> {
    Address::parse(text)
        .ok_or_else(|| Error::Storage("a stored email address is not plain".to_string()))
}

/// An answer Identity's database never writes is a storage failure, never a guess.
fn issue_from(text: &str) -> Result<Issue, Error> {
    Issue::parse(text).ok_or_else(|| Error::Storage(format!("issuing a link answered {text:?}")))
}

/// The event issuing a link records for what it came to.
fn issuance_event(issue: Option<Issue>, id: i64, actor: &Actor) -> (&'static str, String) {
    let reason = match issue {
        Some(Issue::Issued) => None,
        Some(Issue::Disabled) => Some("disabled"),
        Some(Issue::Joined) => Some("joined"),
        None => Some("unknown"),
    };
    reasoned(
        "identity.invite_issued",
        actor,
        reason,
        json!({"admin_id": id}),
    )
}

/// Whether a ceremony was kept: the link may have ended, or its admin changed, since it was read. The function
/// always answers one row, so none is a storage failure.
fn begun(rows: &[Row]) -> Result<bool, Error> {
    rows.first()
        .ok_or_else(answered_nothing)
        .and_then(|row| column(row, "begun"))
}

/// The admin a link names, with the user handle and name a registration needs.
fn invitee_from(row: &Row) -> Result<Invitee, Error> {
    Ok(Invitee {
        admin: AccountId::new(column(row, "admin_id")?),
        user_handle: user_handle(row)?,
        name: column(row, "name")?,
    })
}

/// A ceremony taken to finish. A join always keeps its link, so one without is a storage failure.
fn ceremony_from(row: &Row) -> Result<TakenCeremony, Error> {
    let link: Option<Vec<u8>> = column(row, "link_hash")?;
    Ok(TakenCeremony {
        admin: AccountId::new(column(row, "admin_id")?),
        link_hash: link
            .and_then(|link| link.try_into().ok())
            .ok_or_else(|| Error::Storage("a join ceremony has no link".to_string()))?,
        state: column(row, "state")?,
    })
}

/// Reads what redeeming a link came to, and makes the event it records.
fn redemption(rows: &[Row], id: i64, credential: &[u8], actor: &Actor) -> Decided<Redemption> {
    let redemption = rows
        .first()
        .map_or(Ok(None), |row| column::<Option<String>>(row, "redeemed"))
        .and_then(|answer| answer.as_deref().map(redemption_from).transpose())?;
    Ok((
        redemption,
        Some(redemption_event(redemption, id, credential, actor)),
    ))
}

/// The event a ceremony's outcome is recorded under, refused or not.
fn event_kind(kind: CeremonyKind) -> &'static str {
    match kind {
        CeremonyKind::Join => "identity.invite_redeemed",
        CeremonyKind::Login => "identity.logged_in",
        CeremonyKind::Addition => "identity.passkey_added",
    }
}

/// An answer Identity's database never writes is a storage failure, never a guess.
fn redemption_from(text: &str) -> Result<Redemption, Error> {
    Redemption::parse(text)
        .ok_or_else(|| Error::Storage(format!("redeeming a link answered {text:?}")))
}

/// The event redeeming a link records: changed when the passkey was stored, and refused otherwise, with the reason
/// and the credential ID, which is public, never a secret.
fn redemption_event(
    redemption: Option<Redemption>,
    id: i64,
    credential: &[u8],
    actor: &Actor,
) -> (&'static str, String) {
    let reason = redemption.map_or(Some("unknown"), Redemption::reason);
    reasoned(
        "identity.invite_redeemed",
        actor,
        reason,
        json!({"admin_id": id, "credential": hex(credential)}),
    )
}

/// An admin as listed, with their passkeys and open link.
fn summary_from(row: &Row) -> Result<AdminSummary, Error> {
    let passkeys = count(row, "passkeys")?;
    let link_minutes = column(row, "link_minutes")
        .and_then(|minutes: Option<i64>| minutes.map(nonnegative).transpose())?;
    Ok(AdminSummary {
        id: AccountId::new(column(row, "id")?),
        name: column(row, "name")?,
        created_at: column(row, "created_at")?,
        disabled_at: column(row, "disabled_at")?,
        passkeys,
        link_minutes,
    })
}

/// The admin a create added, or the one already holding its name.
fn added_from(row: &Row) -> Result<(i64, bool), Error> {
    column(row, "id").and_then(|id| column(row, "created").map(|created| (id, created)))
}

/// What a disable did, or nothing for an admin who does not exist.
fn disabling_from(row: &Row) -> Result<Option<Disabling>, Error> {
    let done: Option<bool> = column(row, "done")?;
    let (sessions_ended, links_ended) = (count(row, "sessions_ended")?, count(row, "links_ended")?);
    Ok(done.map(|done| Disabling {
        done,
        sessions_ended,
        links_ended,
    }))
}

/// Reads what removing passkeys did, and makes the event it records: changed when it removed
/// any, unchanged when there were none, and refused for an admin who does not exist.
fn removing(rows: &[Row], id: i64, actor: &Actor) -> Decided<Removal> {
    let removal = rows.first().map(removal_from).transpose()?;
    Ok((
        removal,
        Some(event(
            "identity.passkeys_removed",
            actor,
            outcome(removal.map(|removal| removal.passkeys_removed > 0)),
            json!({
                "admin_id": id,
                "passkeys_removed": removal.map(|removal| removal.passkeys_removed),
                "sessions_ended": removal.map(|removal| removal.sessions_ended),
            }),
        )),
    ))
}

/// A login ceremony's state, if one was taken.
fn taken_state(rows: &[Row]) -> Result<Option<String>, Error> {
    rows.first().map(|row| column(row, "state")).transpose()
}

/// The passkey a credential names, with its admin and their user handle, which must be 16 bytes.
fn signer_from(row: &Row, credential: &[u8]) -> Result<Signer, Error> {
    Ok(Signer {
        credential: credential.to_vec(),
        admin: AccountId::new(column(row, "admin_id")?),
        user_handle: user_handle(row)?,
        passkey: column(row, "passkey")?,
        disabled: column(row, "disabled")?,
    })
}

/// Reads what logging in came to, and makes the event it records: changed when a session
/// started, and refused otherwise, with the reason, the credential ID, which is public, the session's public ID,
/// and the passkey's flags. The function always answers one row, so none is a storage failure.
fn logging_in(rows: &[Row], signer: &Signer, passkey: &str, actor: &Actor) -> Decided<Login> {
    let row = rows.first().ok_or_else(answered_nothing)?;
    let answer: Option<String> = column(row, "outcome")?;
    let login = answer.as_deref().map(login_from).transpose()?;
    let session = session_named(column(row, "session_id")?, login == Some(Login::SignedIn))?;
    let reason = match login {
        Some(Login::SignedIn) => None,
        Some(Login::Disabled) => Some("disabled"),
        Some(Login::Raced) => Some("raced"),
        None => Some("unknown"),
    };
    Ok((
        login,
        Some(reasoned(
            "identity.logged_in",
            actor,
            reason,
            json!({
                "admin_id": signer.admin.get(),
                "credential": hex(&signer.credential),
                "session": session,
                "flags": flags(passkey),
            }),
        )),
    ))
}

/// A passkey's flags as stored, for the audit record: whether it may be backed up, whether it is, and whether its
/// registration verified the user. They come from the passkey Identity just verified, which is always JSON.
fn flags(passkey: &str) -> Value {
    let passkey: Value = serde_json::from_str(passkey).unwrap_or_default();
    let credential = &passkey["cred"];
    json!({
        "backup_eligible": credential["backup_eligible"],
        "backup_state": credential["backup_state"],
        "user_verified": credential["user_verified"],
    })
}

/// A lookup function that always answers one row answered none: a fault of storage, never a guess.
fn answered_nothing() -> Error {
    Error::Storage("a lookup that always answers answered nothing".to_string())
}

/// An answer Identity's database never writes is a storage failure, never a guess.
fn login_from(text: &str) -> Result<Login, Error> {
    Login::parse(text).ok_or_else(|| Error::Storage(format!("logging in answered {text:?}")))
}

/// Bytes as lowercase hexadecimal, as events record credential IDs.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// An account: an invitee's columns from its first row, and a passkey from each row that names one. A passkey
/// whose stored flags are missing is a fault of storage, never a guess.
fn account_from(rows: &[Row]) -> Result<Option<Account>, Error> {
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let invitee = invitee_from(first)?;
    let mut passkeys = Vec::new();
    for row in rows {
        if let Some(credential) = column(row, "credential_id")? {
            passkeys.push(HeldPasskey {
                credential,
                transports: column::<String>(row, "transports")?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                backup_eligible: column(row, "backup_eligible")?,
                backup_state: column(row, "backup_state")?,
            });
        }
    }
    Ok(Some(Account {
        admin: invitee.admin,
        user_handle: invitee.user_handle,
        name: invitee.name,
        passkeys,
    }))
}

/// A stored user handle, which must be 16 bytes: `uuid_send` always writes 16, so any other length is a fault of
/// storage, never a guess.
fn user_handle(row: &Row) -> Result<[u8; 16], Error> {
    let bytes: Vec<u8> = column(row, "user_handle")?;
    bytes
        .try_into()
        .map_err(|_| Error::Storage("a user handle is not 16 bytes".to_string()))
}

/// An addition taken for a session: its admin and state, with the hashes it was taken by.
fn taken_addition(row: &Row, token: &[u8; 32], session: &[u8; 32]) -> Result<TakenAddition, Error> {
    let admin: i64 = column(row, "admin_id")?;
    Ok(TakenAddition {
        token: *token,
        session: *session,
        admin: AccountId::new(admin),
        state: column(row, "state")?,
    })
}

/// Reads what adding a passkey came to, and makes the event it records: changed when the
/// passkey was stored, and refused otherwise, with the reason, the credential ID, the sessions it ended, the adding
/// session's public ID, and the passkey's flags. The function always answers one row, so none is a storage failure.
fn adding(rows: &[Row], id: i64, passkey: &NewPasskey, actor: &Actor) -> Decided<Addition> {
    let row = rows.first().ok_or_else(answered_nothing)?;
    let (addition, ended) = added_and_ended(row)?;
    let session = session_named(
        column(row, "session_id")?,
        addition == Some(Addition::Added),
    )?;
    let reason = addition.map_or(Some("unknown"), Addition::reason);
    Ok((
        addition,
        Some(reasoned(
            "identity.passkey_added",
            actor,
            reason,
            json!({
                "admin_id": id,
                "credential": hex(&passkey.credential_id),
                "sessions_ended": ended,
                "session": session,
                "flags": flags(&passkey.passkey),
            }),
        )),
    ))
}

/// What adding a passkey answered, and how many of the admin's other live sessions it ended.
fn added_and_ended(row: &Row) -> Result<(Option<Addition>, u64), Error> {
    let answer: Option<String> = column(row, "added")?;
    let ended = nonnegative(column(row, "sessions_ended")?)?;
    Ok((answer.as_deref().map(addition_from).transpose()?, ended))
}

/// The public ID of the session a lookup says it started or acted for, which it must name; one it did not is a
/// fault of storage, never a record that names no session.
fn session_named(session: Option<String>, acted: bool) -> Result<Option<String>, Error> {
    if acted && session.is_none() {
        Err(Error::Storage(
            "a lookup that acted for a session named none".to_string(),
        ))
    } else {
        Ok(session)
    }
}

/// An answer Identity's database never writes is a storage failure, never a guess.
fn addition_from(text: &str) -> Result<Addition, Error> {
    Addition::parse(text)
        .ok_or_else(|| Error::Storage(format!("adding a passkey answered {text:?}")))
}

/// What removing passkeys did, from its answer.
fn removal_from(row: &Row) -> Result<Removal, Error> {
    Ok(Removal {
        passkeys_removed: count(row, "passkeys_removed")?,
        sessions_ended: count(row, "sessions_ended")?,
    })
}

/// The outcome of a change: it acted, it found nothing to do, or there was no such admin.
fn outcome(acted: Option<bool>) -> &'static str {
    match acted {
        Some(true) => "changed",
        Some(false) => "unchanged",
        None => "refused",
    }
}

/// Whether a limit admitted an attempt, and, if it did, the event `kind` that records it.
fn admitted_to_record(
    rows: &[Row],
    kind: &'static str,
    actor: &Actor,
    payload: &Value,
) -> Decided<bool> {
    let wait: Option<i64> = rows.first().map_or(Ok(None), |row| column(row, "wait"))?;
    let recorded = wait
        .is_none()
        .then(|| event(kind, actor, "refused", payload.clone()));
    Ok((Some(wait.is_none()), recorded))
}

/// A session as an admin's list shows it, by the id Postgres made for it.
fn listed(row: &Row) -> Result<ListedSession, Error> {
    Ok(ListedSession {
        id: SessionId(column(row, "id")?),
        started_at: column(row, "started_at")?,
        last_seen_at: column(row, "last_seen_at")?,
        device: column(row, "device")?,
        presented: column(row, "presented")?,
    })
}

/// What ending other sessions did, and its record: the admin, the session that asked by its public id, the one
/// it named, if any, and how many ended, or the reason it ended none, as storage gave it. Ending none at all is
/// unchanged, and any reason is refused. A reason Identity's database never writes, or none with no count, is a
/// storage failure, never a guess.
fn ending(rows: &[Row], only: Option<&str>, actor: &Actor) -> Decided<Ending> {
    let row = rows.first().ok_or_else(answered_nothing)?;
    let who: Option<i64> = column(row, "who")?;
    let asking: Option<String> = column(row, "asking")?;
    let reason: Option<String> = column(row, "reason")?;
    let ended = counted(rows, "ended")?;
    // Storage counts only an ending it made, for a session it found, and gives a reason only for none.
    let found = who.is_some() && asking.is_some();
    let (answer, outcome) = match (reason.as_deref(), ended, found) {
        (None, Some(ended), true) => (Ending::Ended(ended), outcome(Some(ended > 0))),
        (Some("stale"), None, true) => (Ending::NotFresh, "refused"),
        (Some("ended" | "disabled"), None, false) => (Ending::NoSession, "refused"),
        _ => {
            return Err(Error::Storage(
                "ending other sessions answered no outcome Identity knows".to_string(),
            ));
        }
    };
    let payload = json!({
        "admin_id": who, "session": asking, "named": only, "ended": ended, "reason": reason,
    });
    let recorded = event("identity.other_sessions_ended", actor, outcome, payload);
    Ok((Some(answer), Some(recorded)))
}

/// What logging out did, and its record: a live session's ending names its account, at the device `actor` acted from,
/// and the session; one already spent, or none, records nothing, since the person ended nothing (#167).
fn logged_out(rows: &[Row], realm: Realm, actor: &Actor) -> Decided<bool> {
    let Some(row) = rows.first() else {
        return Ok((Some(false), None));
    };
    if !column::<bool>(row, "live")? {
        return Ok((Some(false), None));
    }
    let session: String = column(row, "session_id")?;
    let account = AccountId::new(column(row, "account_id")?);
    let named = Actor::browser(realm, Some(account), actor.device());
    let payload = json!({ "admin_id": account.get(), "session": session });
    let recorded = event("identity.logged_out", &named, "changed", payload);
    Ok((Some(true), Some(recorded)))
}

/// A stored realm Identity does not know is an error, never a session in some default realm.
fn session_from(row: &Row, hash: &[u8; 32]) -> Result<Session, Error> {
    let realm: String = column(row, "realm")?;
    let Some(known) = Realm::parse(&realm) else {
        return Err(Error::Storage(format!("unknown realm {realm:?}")));
    };
    Ok(Session {
        hash: *hash,
        realm: known,
        admin: AccountId::new(column(row, "admin_id")?),
        started_at: column(row, "started_at")?,
        proven_at: column(row, "proven_at")?,
        last_seen_at: column(row, "last_seen_at")?,
        ends_at: column(row, "ends_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    mod support {
        use tetsu_postgres::Pool;
        use tetsu_postgres::test_support::{TestDatabase, migrated_database};

        /// A pool logged in as the application role, over a fresh database Identity has migrated, kept while
        /// the test runs.
        pub async fn migrated() -> (Pool, TestDatabase) {
            let database = migrated_database(&[&crate::Identity])
                .await
                .expect("a migrated test database");
            let pool = tetsu_postgres::pool(&database.as_app()).expect("a pool");
            (pool, database)
        }

        /// Who runs the admin command in these tests.
        pub fn actor() -> crate::Actor {
            crate::Actor::parse("kurt@workstation").expect("an actor")
        }
    }

    /// A commit Postgres refused was rolled back, so it is a storage failure, never sorted as worth retrying.
    #[tokio::test]
    async fn a_refused_commit_is_a_storage_failure() {
        let database = tetsu_postgres::test_support::fresh_database()
            .await
            .expect("a test database");
        let client = tetsu_postgres::connect(&database).await.expect("connects");
        let error = client.batch_execute("SELEC").await.expect_err("refused");
        assert_eq!(
            commit_failed(&error),
            Error::Storage("commit refused: syntax error at or near \"SELEC\"".to_string())
        );
    }

    /// Identity holds no row a caller takes by its value, so nothing it stores can be taken today; were one, it
    /// would be a storage failure. Only a temporary failure is worth retrying.
    #[test]
    fn each_failure_has_its_error() {
        assert_eq!(
            failed(Failure::Temporary("dropped".to_string())),
            Error::Unavailable("dropped".to_string())
        );
        assert_eq!(
            failed(Failure::Permanent("refused".to_string())),
            Error::Storage("refused".to_string())
        );
        assert_eq!(
            failed(Failure::Taken {
                constraint: None,
                message: "taken".to_string()
            }),
            Error::Storage("taken".to_string())
        );
    }

    /// Issuing a link records what it came to and why, in this one build, which llvm-cov scores on its own, and
    /// an answer the database never writes is a storage failure.
    #[test]
    fn issuing_a_link_records_what_it_came_to() {
        let recorded = |issue| {
            let (kind, payload) = issuance_event(issue, 7, &support::actor());
            let payload: serde_json::Value = serde_json::from_str(&payload).expect("JSON");
            (kind, payload["outcome"].clone(), payload["reason"].clone())
        };
        let kind = "identity.invite_issued";
        assert_eq!(
            recorded(Some(Issue::Issued)),
            (kind, json!("changed"), json!(null))
        );
        assert_eq!(
            recorded(Some(Issue::Disabled)),
            (kind, json!("refused"), json!("disabled"))
        );
        assert_eq!(
            recorded(Some(Issue::Joined)),
            (kind, json!("refused"), json!("joined"))
        );
        assert_eq!(recorded(None), (kind, json!("refused"), json!("unknown")));
        assert_eq!(issue_from("joined"), Ok(Issue::Joined));
        assert_eq!(
            issue_from("maybe"),
            Err(Error::Storage(
                "issuing a link answered \"maybe\"".to_string()
            ))
        );
    }

    /// A create says what it found and records it, in this one build, which llvm-cov scores on its own: an
    /// admin added, a name another holds, or a name taken by one it cannot yet see.
    #[test]
    fn a_create_reports_and_records_what_it_found() {
        assert_eq!(created(Some((1, true))), Ok(AccountId::new(1)));
        assert_eq!(
            created(Some((1, false))),
            Err(Error::Invalid("admin 1 already has that name".to_string()))
        );
        assert_eq!(
            created(None),
            Err(Error::Invalid(
                "another admin already has that name".to_string()
            ))
        );
        let recorded = |found| {
            let (kind, payload) = creation_event(found, &support::actor());
            let payload: serde_json::Value = serde_json::from_str(&payload).expect("JSON");
            (
                kind,
                payload["outcome"].clone(),
                payload["admin_id"].clone(),
            )
        };
        assert_eq!(
            recorded(Some((1, true))),
            ("identity.admin_created", json!("changed"), json!(1))
        );
        assert_eq!(
            recorded(Some((1, false))),
            ("identity.admin_created", json!("refused"), json!(1))
        );
        assert_eq!(
            recorded(None),
            ("identity.admin_created", json!("refused"), json!(null))
        );
    }

    /// A create's answer with a column of the wrong type is a storage failure, whichever column it is.
    #[tokio::test]
    async fn a_create_answer_of_the_wrong_type_is_a_storage_failure() {
        let (pool, _database) = support::migrated().await;
        let client = pool.get().await.expect("a connection");
        for (answer, column, rust, postgres) in [
            ("SELECT 1::integer AS id, true AS created", 0, "i64", "int4"),
            ("SELECT 1::bigint AS id, 1 AS created", 1, "bool", "int4"),
        ] {
            let rows = client.query(answer, &[]).await.expect("answers");
            assert_eq!(
                added_from(&rows[0]),
                Err(Error::Storage(format!(
                    "error deserializing column {column}: cannot convert between the Rust type `{rust}` and the \
                     Postgres type `{postgres}`"
                )))
            );
        }
    }

    /// A create whose answer cannot be read is a storage failure, in this one build, which llvm-cov scores on its
    /// own, and the admin it added is undone.
    #[tokio::test]
    async fn a_create_answer_that_cannot_be_read_is_a_storage_failure() {
        let (pool, database) = support::migrated().await;
        let db_admin = tetsu_postgres::connect(&database).await.expect("connects");
        db_admin
            .batch_execute("ALTER TABLE identity.admins ALTER COLUMN id TYPE integer")
            .await
            .expect("alters");
        let admins = Accounts::new(PostgresAdmins { pool });
        let failed = admins
            .create(
                &AdminName::parse("Kurt Medley").expect("a name"),
                None,
                &support::actor(),
            )
            .await;
        assert_eq!(
            failed,
            Err(Error::Storage(
                "error deserializing column 0: cannot convert between the Rust type `i64` and the Postgres type \
                 `int4`"
                    .to_string()
            ))
        );
        let left = db_admin
            .query_one("SELECT count(*) FROM identity.admins", &[])
            .await
            .expect("counts");
        assert_eq!(left.try_get::<_, i64>(0).expect("a count"), 0);
    }

    /// A disabled admin is issued no link, and told apart from one who does not exist.
    #[tokio::test]
    async fn a_disabled_admin_is_issued_no_link() {
        let (pool, _database) = support::migrated().await;
        let admins = Accounts::new(PostgresAdmins { pool });
        let kurt = admins
            .create(
                &AdminName::parse("Kurt Medley").expect("a name"),
                None,
                &support::actor(),
            )
            .await
            .expect("creates");
        admins
            .disable(kurt, &support::actor())
            .await
            .expect("disables");
        assert_eq!(
            admins.invite(kurt, &support::actor()).await.map(drop),
            Err(Error::Invalid(format!("no enabled admin {}", kurt.get())))
        );
    }

    /// An admin who holds a passkey is issued no link, in this one build, which llvm-cov scores on its own, and
    /// an answer the database never writes is a storage failure.
    #[tokio::test]
    async fn a_joined_admin_is_issued_no_link() {
        let (pool, database) = support::migrated().await;
        let admins = Accounts::new(PostgresAdmins { pool });
        let lee = admins
            .create(
                &AdminName::parse("Lee Park").expect("a name"),
                None,
                &support::actor(),
            )
            .await
            .expect("creates");
        let db_admin = tetsu_postgres::connect(&database).await.expect("connects");
        db_admin
            .execute(
                "INSERT INTO identity.passkeys (credential_id, admin_id, passkey, created_at)
                 VALUES (sha256('lee'::bytea), $1, '{}', now())",
                &[&lee.get()],
            )
            .await
            .expect("stores a passkey");
        assert_eq!(
            admins.invite(lee, &support::actor()).await.map(drop),
            Err(Error::Invalid(format!(
                "admin {} has a passkey already, and adds another after logging in",
                lee.get()
            )))
        );
        db_admin
            .batch_execute(
                "DROP FUNCTION identity.issue_invite(bytea, bigint);
                 CREATE FUNCTION identity.issue_invite(bytea, bigint) RETURNS text
                 LANGUAGE sql AS 'SELECT ''maybe''::text';
                 GRANT EXECUTE ON FUNCTION identity.issue_invite(bytea, bigint) TO tetsu_identity",
            )
            .await
            .expect("replaces");
        assert_eq!(
            admins.invite(lee, &support::actor()).await.map(drop),
            Err(Error::Storage(
                "issuing a link answered \"maybe\"".to_string()
            ))
        );
    }

    /// Every path through issuing a link, in this one build, which llvm-cov scores on its own: no random source,
    /// a link issued, an unknown admin, and a database that refuses.
    #[tokio::test]
    async fn every_path_through_issuing_a_link() {
        let (pool, database) = support::migrated().await;
        let failing = Accounts::with_fill(PostgresAdmins { pool: pool.clone() }, |_| {
            Err(getrandom::Error::UNSUPPORTED)
        });
        let kurt = failing
            .create(
                &AdminName::parse("Kurt Medley").expect("a name"),
                None,
                &support::actor(),
            )
            .await
            .expect("creates");
        assert_eq!(
            failing.invite(kurt, &support::actor()).await.map(drop),
            Err(Error::Unavailable(format!(
                "no secure random source: {}",
                getrandom::Error::UNSUPPORTED
            )))
        );
        let working = Accounts::with_fill(PostgresAdmins { pool }, getrandom::fill);
        assert!(working.invite(kurt, &support::actor()).await.is_ok());
        assert_eq!(
            working
                .invite(AccountId::new(i64::MAX), &support::actor())
                .await
                .map(drop),
            Err(Error::Invalid(format!("no admin {}", i64::MAX)))
        );
        tetsu_postgres::connect(&database)
            .await
            .expect("connects")
            .batch_execute(
                "REVOKE EXECUTE ON FUNCTION identity.issue_invite(bytea, bigint) FROM tetsu_identity",
            )
            .await
            .expect("revokes");
        assert_eq!(
            working.invite(kurt, &support::actor()).await.map(drop),
            Err(Error::Storage(
                "permission denied for function issue_invite".to_string()
            ))
        );
    }
}
