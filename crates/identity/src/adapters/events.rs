//! Identity's notices (ADR-0074): an outbox consumer of its own events that emails an operator when they log in or
//! add a passkey, so a login they did not make is one they hear about (`v5.0.0-6.3.5`, `v5.0.0-6.3.7`). It applies
//! each event in the transaction the dispatcher opens as Identity's role, which reads the operator's address.

use std::sync::Arc;

use serde_json::Value;
use tetsu_kernel::{Delivery, MailError, Mailer};
use tetsu_postgres::{Applied, Consumer, Event, Transaction};

use crate::mail::{Notice, notice_message};

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

    fn apply<'a>(&'a self, transaction: &'a Transaction<'a>, event: &'a Event) -> Applied<'a> {
        Box::pin(async move {
            let told = changed_operator(&event.payload).zip(Notice::of(&event.kind));
            let Some((operator, notice)) = told else {
                return Ok(());
            };
            let found = transaction
                .query_opt(NOTICE, &[&operator, &event.occurred_at])
                .await?;
            let message = found.and_then(|row| {
                row.try_get::<_, Option<String>>("email")
                    .ok()
                    .flatten()
                    .zip(row.try_get::<_, String>("occurred").ok())
                    .and_then(|(email, at)| notice_message(notice, &email, &at, &event.id))
            });
            match message {
                Some(message) => sent(self.mailer.send(&message).await),
                None => Ok(()),
            }
        })
    }
}

/// The operator an event's payload names, if what it records went through: a refused login or addition is no
/// one's to be told of.
fn changed_operator(payload: &str) -> Option<i64> {
    let payload: Value = serde_json::from_str(payload).ok()?;
    (payload["outcome"] == "changed")
        .then(|| payload["operator_id"].as_i64())
        .flatten()
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
        for (payload, operator) in [
            (r#"{"outcome": "changed", "operator_id": 7}"#, Some(7)),
            (r#"{"outcome": "refused", "operator_id": 7}"#, None),
            (r#"{"outcome": "changed"}"#, None),
            (r#"{"outcome": "changed", "operator_id": "7"}"#, None),
            ("not json", None),
        ] {
            assert_eq!(changed_operator(payload), operator, "{payload}");
        }
    }

    #[test]
    fn only_a_notice_that_could_not_go_just_now_is_tried_again() {
        assert!(sent(Err(MailError::Unavailable)).is_err());
        for done in [
            Ok(Delivery::Sent),
            Ok(Delivery::Captured),
            Ok(Delivery::Repeated),
            Err(MailError::Refused),
        ] {
            assert!(sent(done).is_ok(), "{done:?}");
        }
    }
}
