//! Identity's notices (ADR-0074): an outbox consumer of its own events that emails an operator when they log in, add
//! a passkey, or join, naming when and from which device, so a login they did not make is one they hear about
//! (`v5.0.0-6.3.5`, `v5.0.0-6.3.7`). It applies
//! each event in the transaction the dispatcher opens as Identity's role, which reads the operator's address.
//!
//! Identity's inbox records each event a consumer of Identity's applied, by the event alone, so this is the one
//! consumer Identity runs; a second would find these events already applied.

use std::sync::Arc;

use serde_json::Value;
use tetsu_kernel::{Delivery, MailError, Mailer};
use tetsu_postgres::{Applied, Consumer, Event, Transaction};

use crate::mail::{Happened, Notice, notice_message};

const NOTICE: &str = include_str!("../../sql/notice.sql");

/// Emails operators through `mailer` when they log in or add a passkey.
#[must_use]
pub fn notices<M: Mailer + 'static>(mailer: Arc<M>) -> Arc<dyn Consumer> {
    Arc::new(Notices { mailer })
}

struct Notices<M> {
    mailer: Arc<M>,
}

impl<M: Mailer + 'static> Consumer for Notices<M> {
    fn schema(&self) -> &'static str {
        "identity"
    }

    fn wants(&self, event: &Event) -> bool {
        event.producer == "identity" && Notice::of(&event.kind).is_some()
    }

    /// A security notice is worth sending late, so one mail cannot send for keeps retrying hourly, as the audit
    /// record's events do, and is reported when it runs out of attempts (ADR-0045).
    fn never_lose(&self) -> bool {
        true
    }

    fn apply<'a>(&'a self, transaction: &'a Transaction<'a>, event: &'a Event) -> Applied<'a> {
        Box::pin(async move {
            let told = changed(&event.payload).zip(Notice::of(&event.kind));
            let Some(((operator, device), notice)) = told else {
                return Ok(());
            };
            let Some(row) = transaction
                .query_opt(NOTICE, &[&operator, &event.occurred_at])
                .await?
            else {
                return Ok(());
            };
            // An address that cannot be read is a fault to try again; an operator with none has no one to tell.
            let Some(email) = row.try_get::<_, Option<String>>("email")? else {
                return Ok(());
            };
            // Computed as text by the query, so it always reads.
            let at: String = row.try_get("occurred").unwrap_or_default();
            let happened = Happened {
                at: &at,
                device: &device,
            };
            let Some(message) = notice_message(notice, &email, &happened, &event.id) else {
                tracing::warn!(
                    operator,
                    "identity: an operator's address is not plain, so they get no notices"
                );
                return Ok(());
            };
            sent(self.mailer.send(&message).await)
        })
    }
}

/// The operator an event's payload names, with the device that acted, if what it records went through: a refused
/// login or addition is no one's to be told of. The actor reads `operator-<id>@<device>`, and a device the request
/// did not name reads as unknown.
fn changed(payload: &str) -> Option<(i64, String)> {
    let payload: Value = serde_json::from_str(payload).ok()?;
    let device = payload["actor"]
        .as_str()
        .and_then(|actor| actor.split_once('@'))
        .map(|(_, device)| device)
        .filter(|device| *device != "unknown")
        .unwrap_or("an unknown device");
    (payload["outcome"] == "changed")
        .then(|| payload["operator_id"].as_i64())
        .flatten()
        .map(|operator| (operator, device.to_string()))
}

/// A notice that could not go just now is tried again; one the mail server refused for good, or one sent or kept,
/// is done.
fn sent(sent: Result<Delivery, MailError>) -> Result<(), tetsu_postgres::Error> {
    match sent {
        Err(MailError::Unavailable) => Err("mail is unavailable".into()),
        Ok(_) | Err(MailError::Refused) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_change_names_an_operator_to_tell() {
        let unknown = || Some((7, "an unknown device".to_string()));
        for (payload, told) in [
            (
                r#"{"outcome": "changed", "operator_id": 7, "actor": "operator-7@Firefox on Linux"}"#,
                Some((7, "Firefox on Linux".to_string())),
            ),
            (
                r#"{"outcome": "changed", "operator_id": 7, "actor": "operator-7@unknown"}"#,
                unknown(),
            ),
            (
                r#"{"outcome": "changed", "operator_id": 7, "actor": "no device"}"#,
                unknown(),
            ),
            (r#"{"outcome": "changed", "operator_id": 7}"#, unknown()),
            (r#"{"outcome": "refused", "operator_id": 7}"#, None),
            (r#"{"outcome": "changed"}"#, None),
            (r#"{"outcome": "changed", "operator_id": "7"}"#, None),
            ("not json", None),
        ] {
            assert_eq!(changed(payload), told, "{payload}");
        }
    }

    #[test]
    fn only_a_notice_that_could_not_go_just_now_is_tried_again() {
        assert!(sent(Err(MailError::Unavailable)).is_err());
        for done in [
            Ok(Delivery::Sent),
            Ok(Delivery::Captured),
            Ok(Delivery::Repeated),
            Ok(Delivery::Withheld),
            Err(MailError::Refused),
        ] {
            assert!(sent(done).is_ok(), "{done:?}");
        }
    }
}
