//! The only ways code reaches a module's tables (ADR-0042). Each opens a transaction and sets everything it needs
//! with transaction scope in one round trip, so commit or rollback clears it before the pool hands the connection
//! to the next request (#53).

use std::sync::atomic::{AtomicBool, Ordering};

use deadpool_postgres::{Object, Transaction};

use crate::roles::{DELIVERY_ROLE, module_role};

/// A query's limit inside a transaction, the same as the pool's own (ADR-0011).
pub const STATEMENT_TIMEOUT: &str = "5s";
/// How long a statement waits for a lock before failing, so one slow transaction cannot queue every request.
pub const LOCK_TIMEOUT: &str = "2s";
/// How long a request's whole transaction may last, and sit idle within it, before Postgres ends the session, so an
/// open transaction never holds back vacuum or its locks for long (ADR-0060). Postgres arms a transaction limit set
/// inside the transaction, so it is set here, never on the session, where a later one could not tighten it.
pub const TRANSACTION_TIMEOUT: &str = "15s";
pub const IDLE_TIMEOUT: &str = "10s";
/// The delivery role's limits cover its longest pass: a minute, then at most one more consumer's wait for a
/// connection and its 30 seconds, since a pass stops before any later consumer once its minute is up.
const DELIVERY_TIMEOUT: &str = "2min";

/// Clears the tenant too, so a setting left on the session, however it got there, never narrows a module
/// transaction to a tenant it did not choose.
const SET_MODULE: &str = "SELECT pg_catalog.set_config('role', $1, true),
       pg_catalog.set_config('statement_timeout', $2, true),
       pg_catalog.set_config('lock_timeout', $3, true),
       pg_catalog.set_config('transaction_timeout', $4, true),
       pg_catalog.set_config('idle_in_transaction_session_timeout', $5, true),
       pg_catalog.set_config('app.tenant_id', '', true)";
const SET_TENANT_TOO: &str = "SELECT pg_catalog.set_config('role', $1, true),
       pg_catalog.set_config('statement_timeout', $2, true),
       pg_catalog.set_config('lock_timeout', $3, true),
       pg_catalog.set_config('transaction_timeout', $4, true),
       pg_catalog.set_config('idle_in_transaction_session_timeout', $5, true),
       pg_catalog.set_config('app.tenant_id', $6, true)";
const SET_TENANT: &str = "SELECT pg_catalog.set_config('app.tenant_id', $1, true)";
/// What a module transaction sets, as the delivery role, which may reach no tenant through a setting either.
const SET_DELIVERY: &str = SET_MODULE;

/// A transaction acting as one module's role, with no tenant: it reaches global tables and lookup functions, and
/// tenant-owned tables show it no rows.
pub struct ModuleTransaction<'a> {
    scoped: Scoped<'a>,
}

/// A transaction acting as one module's role for one tenant. Tenant-owned tables show it that tenant's rows alone.
pub struct TenantTransaction<'a> {
    scoped: Scoped<'a>,
}

/// A transaction as the delivery role, which reads and settles every module's outboxes and reaches nothing else
/// (ADR-0043).
pub struct DeliveryTransaction<'a> {
    transaction: Transaction<'a>,
}

/// A module's transaction, the schema it acts in, and whether it recorded an event, which wakes the dispatcher
/// once it commits.
struct Scoped<'a> {
    transaction: Transaction<'a>,
    schema: String,
    recorded: AtomicBool,
}

impl<'a> Scoped<'a> {
    fn new(transaction: Transaction<'a>, schema: &str) -> Self {
        Self {
            transaction,
            schema: schema.to_string(),
            recorded: AtomicBool::new(false),
        }
    }

    async fn commit(self) -> Result<(), tokio_postgres::Error> {
        self.transaction.commit().await?;
        if self.recorded.load(Ordering::Relaxed) {
            crate::events::wake();
        }
        Ok(())
    }
}

impl<'a> ModuleTransaction<'a> {
    /// # Errors
    /// Fails if the transaction cannot start, or the connection's role may not act as the module's.
    pub async fn begin(
        client: &'a mut Object,
        schema: &str,
    ) -> Result<Self, tokio_postgres::Error> {
        let transaction = open(
            client,
            SET_MODULE,
            &[
                &module_role(schema),
                &STATEMENT_TIMEOUT,
                &LOCK_TIMEOUT,
                &TRANSACTION_TIMEOUT,
                &IDLE_TIMEOUT,
            ],
        )
        .await?;
        Ok(Self {
            scoped: Scoped::new(transaction, schema),
        })
    }

    /// Narrows this transaction to one tenant, such as once a lookup has found it.
    ///
    /// # Errors
    /// Fails if the setting cannot be made.
    pub async fn for_tenant(
        self,
        tenant: i64,
    ) -> Result<TenantTransaction<'a>, tokio_postgres::Error> {
        let statement = self.scoped.transaction.prepare_cached(SET_TENANT).await?;
        self.scoped
            .transaction
            .query_one(&statement, &[&tenant.to_string()])
            .await?;
        Ok(TenantTransaction {
            scoped: self.scoped,
        })
    }
}

impl<'a> TenantTransaction<'a> {
    /// # Errors
    /// Fails if the transaction cannot start, or the connection's role may not act as the module's.
    pub async fn begin(
        client: &'a mut Object,
        schema: &str,
        tenant: i64,
    ) -> Result<Self, tokio_postgres::Error> {
        let transaction = open(
            client,
            SET_TENANT_TOO,
            &[
                &module_role(schema),
                &STATEMENT_TIMEOUT,
                &LOCK_TIMEOUT,
                &TRANSACTION_TIMEOUT,
                &IDLE_TIMEOUT,
                &tenant.to_string(),
            ],
        )
        .await?;
        Ok(Self {
            scoped: Scoped::new(transaction, schema),
        })
    }
}

impl<'a> DeliveryTransaction<'a> {
    /// # Errors
    /// Fails if the transaction cannot start, or the connection's role may not act as the delivery role.
    pub async fn begin(client: &'a mut Object) -> Result<Self, tokio_postgres::Error> {
        let transaction = open(
            client,
            SET_DELIVERY,
            &[
                &DELIVERY_ROLE,
                &STATEMENT_TIMEOUT,
                &LOCK_TIMEOUT,
                &DELIVERY_TIMEOUT,
                &DELIVERY_TIMEOUT,
            ],
        )
        .await?;
        Ok(Self { transaction })
    }

    /// # Errors
    /// Fails if Postgres refuses the commit, or does not answer.
    pub async fn commit(self) -> Result<(), tokio_postgres::Error> {
        self.transaction.commit().await
    }
}

/// Starts a transaction and makes its settings in one round trip.
async fn open<'a>(
    client: &'a mut Object,
    settings: &str,
    values: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Transaction<'a>, tokio_postgres::Error> {
    let transaction = client.transaction().await?;
    let statement = transaction.prepare_cached(settings).await?;
    transaction.query_one(&statement, values).await?;
    Ok(transaction)
}

/// What both module transactions offer: commit, their schema, and the transaction itself.
macro_rules! module_transaction {
    ($name:ident) => {
        impl<'a> $name<'a> {
            /// # Errors
            /// Fails if Postgres refuses the commit, or does not answer.
            pub async fn commit(self) -> Result<(), tokio_postgres::Error> {
                self.scoped.commit().await
            }

            /// The schema of the module this transaction acts as.
            #[must_use]
            pub fn schema(&self) -> &str {
                &self.scoped.schema
            }

            /// Marks that this transaction recorded an event, so its commit wakes the dispatcher.
            pub(crate) fn recorded(&self) {
                self.scoped.recorded.store(true, Ordering::Relaxed);
            }
        }

        impl<'a> std::ops::Deref for $name<'a> {
            type Target = Transaction<'a>;

            fn deref(&self) -> &Self::Target {
                &self.scoped.transaction
            }
        }
    };
}

module_transaction!(ModuleTransaction);
module_transaction!(TenantTransaction);

impl<'a> std::ops::Deref for DeliveryTransaction<'a> {
    type Target = Transaction<'a>;

    fn deref(&self) -> &Self::Target {
        &self.transaction
    }
}
