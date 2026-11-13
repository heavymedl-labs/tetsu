//! Audit's inbound adapter: each event it wants becomes a row of the record, in the transaction the dispatcher opens
//! as Audit's role for the event's tenant.

use std::sync::Arc;

use tetsu_postgres::{Applied, Consumer, Event, Transaction};

const RECORD: &str = include_str!("../../sql/record.sql");
const RECORD_GLOBAL: &str = include_str!("../../sql/record_global.sql");

/// Records every event from the modules whose schemas are `producers`, such as Identity's.
#[must_use]
pub fn consumer(producers: &'static [&'static str]) -> Arc<dyn Consumer> {
    Arc::new(Recorder { producers })
}

struct Recorder {
    producers: &'static [&'static str],
}

impl Consumer for Recorder {
    fn schema(&self) -> &'static str {
        "audit"
    }

    fn wants(&self, event: &Event) -> bool {
        self.producers.contains(&event.producer.as_str())
    }

    /// The audit record may not be lost (ADR-0045).
    fn never_lose(&self) -> bool {
        true
    }

    fn apply<'a>(&'a self, transaction: &'a Transaction<'a>, event: &'a Event) -> Applied<'a> {
        Box::pin(async move {
            let fields: [&(dyn tokio_postgres::types::ToSql + Sync); 6] = [
                &event.id,
                &event.producer,
                &event.kind,
                &event.version,
                &event.occurred_at,
                &event.payload,
            ];
            match &event.tenant {
                Some(tenant) => {
                    let mut values = fields.to_vec();
                    values.push(tenant);
                    transaction.execute(RECORD, &values).await?
                }
                None => transaction.execute(RECORD_GLOBAL, &fields).await?,
            };
            Ok(())
        })
    }
}
