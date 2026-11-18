//! The post office: Mail's `Mailer` over Postgres. It claims each key before acting, so a message goes once, then
//! keeps the message for the mailbox page or hands it to SMTP. Driver types stay in this file (ADR-0007).

use std::time::SystemTime;

use deadpool_postgres::Transaction;
use tetsu_kernel::{Captured, Delivery, MailError, Mailbox, Mailer, Message, Reading};
use tetsu_postgres::{Failure, ModuleTransaction, Pool};
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;

use super::smtp::Smtp;
use crate::domain::Allowlist;

/// Mail's schema, which names the role its transactions act as (ADR-0042).
const SCHEMA: &str = "mail";

const CLAIM: &str = include_str!("../../sql/claim.sql");
const RELEASE: &str = include_str!("../../sql/release.sql");
const CAPTURE: &str = include_str!("../../sql/capture.sql");
const FORGET: &str = include_str!("../../sql/forget.sql");
const MAILBOX: &str = include_str!("../../sql/mailbox.sql");

/// Mail over Postgres, for the composition root: with `smtp`, messages to an address on `allowlist` go out by SMTP,
/// and every other message is kept for the mailbox page.
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
        match &self.smtp {
            Some(smtp) if self.allowlist.contains(message.to()) => self.relay(smtp, message).await,
            _ => self.keep(message).await,
        }
    }
}

/// The newest hundred messages kept for the mailbox page, newest first.
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

impl PostOffice {
    /// Claims the key, then sends; a failed send gives the key back, so a later attempt may try again. A process
    /// that stops between sending and that answer leaves the key claimed, so the message is never sent twice.
    async fn relay(&self, smtp: &Smtp, message: &Message) -> Result<Delivery, MailError> {
        let claimed = self
            .transact(async |transaction| claim(transaction, message, "sent").await)
            .await?;
        if !claimed {
            return Ok(Delivery::Repeated);
        }
        match smtp.deliver(message).await {
            Ok(()) => Ok(Delivery::Sent),
            Err(error) => {
                self.transact(async |transaction| {
                    transaction.execute(RELEASE, &[&message.key()]).await
                })
                .await?;
                Err(error)
            }
        }
    }

    /// Claims the key, keeps the message, and forgets kept messages older than a week, in one transaction.
    async fn keep(&self, message: &Message) -> Result<Delivery, MailError> {
        self.transact(async |transaction| {
            if !claim(transaction, message, "captured").await? {
                return Ok(Delivery::Repeated);
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
            Ok(Delivery::Captured)
        })
        .await
    }

    /// Runs `work` in one transaction as Mail's role. Every statement goes through here, so each way Postgres can
    /// fail is handled once: before the work, in it, and at commit.
    async fn transact<T>(
        &self,
        work: impl AsyncFnOnce(&Transaction<'_>) -> Result<T, tokio_postgres::Error>,
    ) -> Result<T, MailError> {
        let mut client = self
            .pool
            .get()
            .await
            .map_err(|e| failure(&tetsu_postgres::sort_pool(e)))?;
        let attempt = async {
            let transaction = ModuleTransaction::begin(&mut client, SCHEMA)
                .await
                .map_err(Stage::Work)?;
            let value = work(&transaction).await.map_err(Stage::Work)?;
            transaction.commit().await.map_err(Stage::Commit)?;
            Ok(value)
        };
        attempt.await.map_err(|stage| match stage {
            Stage::Work(error) => failure(&tetsu_postgres::sort(&error)),
            Stage::Commit(error) => failure(&tetsu_postgres::sort_commit(&error)),
        })
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

/// Takes `message`'s key for `route`, and says whether it was free.
async fn claim(
    transaction: &Transaction<'_>,
    message: &Message,
    route: &str,
) -> Result<bool, tokio_postgres::Error> {
    Ok(transaction
        .query_opt(CLAIM, &[&message.key(), &route])
        .await?
        .is_some())
}

/// Where a transaction failed, since an error at commit may mean it committed after all (ADR-0011).
enum Stage {
    Work(tokio_postgres::Error),
    Commit(tokio_postgres::Error),
}

/// Sorts a driver failure (ADR-0011): what may pass is worth trying again, and the rest is not.
fn failure(failure: &Failure) -> MailError {
    match failure {
        Failure::Temporary(_) => MailError::Unavailable,
        Failure::Taken { .. } | Failure::Permanent(_) => MailError::Refused,
    }
}
