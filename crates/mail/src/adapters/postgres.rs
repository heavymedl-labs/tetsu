//! The post office: Mail's `Mailer` over Postgres. It claims each key before acting, so a message goes once, then
//! keeps the message for the mailbox page or hands it to SMTP, and settles the key with what came of it, or gives it
//! back when the server did not take the message. A claim left sending, by a stop or a send whose outcome is not
//! known, may be taken again after two minutes, so no message is stranded. Driver types stay in this file
//! (ADR-0007).

use std::time::SystemTime;

use deadpool_postgres::Transaction;
use tetsu_kernel::{Captured, Delivery, MailError, Mailbox, Mailer, Message, Reading};
use tetsu_postgres::{Failure, ModuleTransaction, Pool};
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;

use super::smtp::{Failed, Smtp};
use crate::domain::Allowlist;

/// Mail's schema, which names the role its transactions act as (ADR-0042).
const SCHEMA: &str = "mail";

const CLAIM: &str = include_str!("../../sql/claim.sql");
const SETTLE: &str = include_str!("../../sql/settle.sql");
const CAPTURE: &str = include_str!("../../sql/capture.sql");
const FORGET: &str = include_str!("../../sql/forget.sql");
const MAILBOX: &str = include_str!("../../sql/mailbox.sql");

/// Mail over Postgres, for the composition root: with `smtp`, messages to an address on `allowlist` go out by SMTP,
/// and every other message is kept for the mailbox page, unless it may never be kept.
#[must_use]
pub fn post_office(pool: Pool, smtp: Option<Smtp>, allowlist: Allowlist) -> PostOffice {
    PostOffice {
        pool,
        smtp,
        allowlist,
    }
}

/// Sends or keeps each message once, and reads back what it kept.
pub struct PostOffice {
    pool: Pool,
    smtp: Option<Smtp>,
    allowlist: Allowlist,
}

impl Mailer for PostOffice {
    async fn send(&self, message: &Message) -> Result<Delivery, MailError> {
        let delivery = match &self.smtp {
            Some(smtp) if self.allowlist.contains(message.to()) => self.relay(smtp, message).await,
            _ if message.may_be_kept() => self.keep(message).await,
            _ => Ok(Delivery::Withheld),
        };
        if let Err(error) = delivery {
            // The key's kind alone, such as invite or notice, never the address, nor a key with no kind.
            let kind = message
                .key()
                .split_once(':')
                .map_or("other", |(kind, _)| kind);
            tracing::warn!(kind, ?error, "mail: a message did not go");
        }
        delivery
    }
}

/// The newest hundred messages kept in the past week, newest first.
impl Mailbox for PostOffice {
    fn captured(&self) -> Reading<'_> {
        Box::pin(self.transact(async |transaction| {
            transaction
                .query(MAILBOX, &[])
                .await?
                .iter()
                .map(captured)
                .collect()
        }))
    }
}

/// How a key stood when a message was handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// This attempt holds it.
    Taken,
    /// Another attempt is sending it now.
    Busy,
    /// It was settled before.
    Settled,
}

impl PostOffice {
    /// Claims the key, sends, and settles it as sent, or refused for good. A send the server did not take gives the
    /// key back, so the next attempt tries again at once. One whose outcome is not known leaves the claim to lapse,
    /// so a message the server did take goes at most twice, which its `Message-ID` lets a reader see. Settling also
    /// forgets what is past keeping, so a store that only relays is cleaned too.
    async fn relay(&self, smtp: &Smtp, message: &Message) -> Result<Delivery, MailError> {
        match self
            .transact(async |transaction| claim(transaction, message, "sending").await)
            .await?
        {
            Claim::Taken => {}
            Claim::Busy => return Err(MailError::Unavailable),
            Claim::Settled => return Ok(Delivery::Repeated),
        }
        // How the key is settled, with no route giving it back, and what the caller is told.
        let (route, answer) = match smtp.deliver(message).await {
            Ok(()) => (Some("sent"), Ok(Delivery::Sent)),
            Err(Failed::Refused) => (Some("refused"), Err(MailError::Refused)),
            Err(Failed::NotTaken) => (None, Err(MailError::Unavailable)),
            Err(Failed::Unknown) => return Err(MailError::Unavailable),
        };
        let recorded = self
            .transact(async |transaction| {
                transaction
                    .execute(SETTLE, &[&message.key(), &route])
                    .await?;
                transaction.execute(FORGET, &[]).await
            })
            .await;
        if recorded.is_err() {
            tracing::warn!("mail: a key could not be settled, so its claim lapses in two minutes");
        }
        answer
    }

    /// Claims the key, keeps the message, and forgets what is past keeping, in one transaction.
    async fn keep(&self, message: &Message) -> Result<Delivery, MailError> {
        self.transact(async |transaction| {
            match claim(transaction, message, "captured").await? {
                Claim::Taken => {}
                Claim::Busy => return Ok(Err(MailError::Unavailable)),
                Claim::Settled => return Ok(Ok(Delivery::Repeated)),
            }
            let fields: [&(dyn ToSql + Sync); 5] = [
                &message.key(),
                &message.to().as_str(),
                &message.subject(),
                &message.text(),
                &message.html(),
            ];
            transaction.execute(CAPTURE, &fields).await?;
            transaction.execute(FORGET, &[]).await?;
            Ok(Ok(Delivery::Captured))
        })
        .await?
    }

    /// Runs `work` in one transaction as Mail's role. Every statement goes through here, so each way Postgres can
    /// fail is handled once: before the work, in it, and at commit. Each is worth trying again, being an outage or a
    /// fault in how Mail is set up rather than in the message, and is logged by what Postgres said, which names no
    /// row's values. A stale cached statement clears the pool's caches.
    async fn transact<T>(
        &self,
        work: impl AsyncFnOnce(&Transaction<'_>) -> Result<T, tokio_postgres::Error>,
    ) -> Result<T, MailError> {
        let mut client = self
            .pool
            .get()
            .await
            .map_err(|e| self.failed(&tetsu_postgres::sort_pool(e)))?;
        let attempt = async {
            let transaction = ModuleTransaction::begin(&mut client, SCHEMA)
                .await
                .map_err(Stage::Work)?;
            let value = work(&transaction).await.map_err(Stage::Work)?;
            transaction.commit().await.map_err(Stage::Commit)?;
            Ok(value)
        };
        attempt.await.map_err(|stage| match stage {
            Stage::Work(error) => self.failed(&tetsu_postgres::sort(&error)),
            Stage::Commit(error) => self.failed(&tetsu_postgres::sort_commit(&error)),
        })
    }

    fn failed(&self, failure: &Failure) -> MailError {
        if let Failure::Temporary(why) = failure {
            tetsu_postgres::clear_if_stale(&self.pool, why);
        }
        tracing::warn!(?failure, "mail: the store failed");
        MailError::Unavailable
    }
}

/// A kept message as the mailbox reads it.
fn captured(row: &Row) -> Result<Captured, tokio_postgres::Error> {
    Ok(Captured {
        recipient: row.try_get("recipient")?,
        subject: row.try_get("subject")?,
        text: row.try_get("text_body")?,
        html: row.try_get("html_body")?,
        at: row.try_get::<_, SystemTime>("captured_at")?,
    })
}

/// Takes `message`'s key for `route`, and says how it stood.
async fn claim(
    transaction: &Transaction<'_>,
    message: &Message,
    route: &str,
) -> Result<Claim, tokio_postgres::Error> {
    let row = transaction
        .query_one(CLAIM, &[&message.key(), &route])
        .await?;
    // Both columns are computed, so neither fails to read; were one to, the answer is to try later, never that the
    // message went.
    let taken = row.try_get::<_, bool>("taken").ok();
    let before = row.try_get::<_, Option<String>>("route").ok().flatten();
    Ok(match (taken, before.as_deref()) {
        (Some(true), _) => Claim::Taken,
        (Some(false), Some("sent" | "captured" | "refused")) => Claim::Settled,
        _ => Claim::Busy,
    })
}

/// Where a transaction failed, since an error at commit may mean it committed after all (ADR-0011).
enum Stage {
    Work(tokio_postgres::Error),
    Commit(tokio_postgres::Error),
}
