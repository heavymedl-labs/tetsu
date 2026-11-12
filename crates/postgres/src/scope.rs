//! The only ways code reaches a module's tables (ADR-0042). Each opens a transaction and sets everything it needs
//! with transaction scope in one round trip, so commit or rollback clears it before the pool hands the connection
//! to the next request (#53).

use deadpool_postgres::{Object, Transaction};

use crate::roles::module_role;

/// A query's limit inside a transaction, the same as the pool's own (ADR-0011).
pub const STATEMENT_TIMEOUT: &str = "5s";
/// How long a statement waits for a lock before failing, so one slow transaction cannot queue every request.
pub const LOCK_TIMEOUT: &str = "2s";

const SET_MODULE: &str = "SELECT pg_catalog.set_config('role', $1, true),
       pg_catalog.set_config('statement_timeout', $2, true),
       pg_catalog.set_config('lock_timeout', $3, true)";
const SET_TENANT_TOO: &str = "SELECT pg_catalog.set_config('role', $1, true),
       pg_catalog.set_config('statement_timeout', $2, true),
       pg_catalog.set_config('lock_timeout', $3, true),
       pg_catalog.set_config('app.tenant_id', $4, true)";
const SET_TENANT: &str = "SELECT pg_catalog.set_config('app.tenant_id', $1, true)";

/// A transaction acting as one module's role, with no tenant: it reaches global tables and lookup functions, and
/// tenant-owned tables show it no rows.
pub struct ModuleTransaction<'a> {
    transaction: Transaction<'a>,
}

/// A transaction acting as one module's role for one tenant. Tenant-owned tables show it that tenant's rows alone.
pub struct TenantTransaction<'a> {
    transaction: Transaction<'a>,
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
            &[&module_role(schema), &STATEMENT_TIMEOUT, &LOCK_TIMEOUT],
        )
        .await?;
        Ok(Self { transaction })
    }

    /// Narrows this transaction to one tenant, such as once a lookup has found it.
    ///
    /// # Errors
    /// Fails if the setting cannot be made.
    pub async fn for_tenant(
        self,
        tenant: i64,
    ) -> Result<TenantTransaction<'a>, tokio_postgres::Error> {
        let statement = self.transaction.prepare_cached(SET_TENANT).await?;
        self.transaction
            .query_one(&statement, &[&tenant.to_string()])
            .await?;
        Ok(TenantTransaction {
            transaction: self.transaction,
        })
    }

    /// # Errors
    /// Fails if Postgres refuses the commit, or does not answer.
    pub async fn commit(self) -> Result<(), tokio_postgres::Error> {
        self.transaction.commit().await
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
                &tenant.to_string(),
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

impl<'a> std::ops::Deref for ModuleTransaction<'a> {
    type Target = Transaction<'a>;

    fn deref(&self) -> &Self::Target {
        &self.transaction
    }
}

impl<'a> std::ops::Deref for TenantTransaction<'a> {
    type Target = Transaction<'a>;

    fn deref(&self) -> &Self::Target {
        &self.transaction
    }
}
