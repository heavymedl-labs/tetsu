//! Audit's inbound adapter: each event it wants becomes a row of the record, in the dispatcher's transaction, which
//! acts as Audit's role for the event's tenant.

use std::sync::Arc;

use tetsu_postgres::{Applied, Consumer, Event, Transaction};

const RECORD: &str = "INSERT INTO audit.records
    (tenant_id, event_id, producer, event_type, event_version, occurred_at, payload)
    VALUES ((SELECT nullif(current_setting('app.tenant_id', true), '')::bigint),
            $1::text::uuid, $2, $3, $4, $5, $6::text::jsonb)";
const RECORD_GLOBAL: &str = "INSERT INTO audit.records_global
    (event_id, producer, event_type, event_version, occurred_at, payload)
    VALUES ($1::text::uuid, $2, $3, $4, $5, $6::text::jsonb)";

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

    fn apply<'a>(&'a self, transaction: &'a Transaction<'a>, event: &'a Event) -> Applied<'a> {
        Box::pin(async move {
            let sql = if event.tenant.is_some() {
                RECORD
            } else {
                RECORD_GLOBAL
            };
            transaction
                .execute(
                    sql,
                    &[
                        &event.id,
                        &event.producer,
                        &event.kind,
                        &event.version,
                        &event.occurred_at,
                        &event.payload,
                    ],
                )
                .await?;
            Ok(())
        })
    }
}
