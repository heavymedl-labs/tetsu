//! Use cases, reached only through [`Tenants`] (ADR-0021).

use crate::domain::{Error, NewTenant, Slug, Tenant};
use crate::ports::TenantStore;

/// Tenancy's use cases over a store. It offers nothing that reaches the store, so a caller outside Tenancy can
/// never skip a use case, such as `resolve`'s checks, through it. Tenancy's tables stay reachable through the
/// database pool, which the rule that a module touches only its own schema governs (#8). The doc tests on
/// `postgres_store` rule out a store impl, a deref, and a public field. Review catches any other route to the
/// store, such as a method or a conversion trait that hands it out.
pub struct Tenants<S> {
    store: S,
}

impl<S: TenantStore> Tenants<S> {
    /// Wraps a store, such as a test's own or another adapter.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Creates a tenant, reachable at its slug.
    ///
    /// # Errors
    /// Fails if the slug is taken or storage fails.
    pub async fn create(&self, tenant: &NewTenant) -> Result<Tenant, Error> {
        self.store.insert(tenant).await
    }

    /// Finds the tenant at an address. An unknown address fails, and never falls back to a default tenant.
    ///
    /// # Errors
    /// Fails with [`Error::NotFound`] for an unknown address, including one holding a control character, which
    /// no address does. Fails if storage fails.
    pub async fn resolve(&self, address: &str) -> Result<Tenant, Error> {
        let address = normalized(address).ok_or(Error::NotFound)?;
        self.store
            .find_by_address(&address)
            .await?
            .ok_or(Error::NotFound)
    }

    /// The name of the business with an id, for a page that knows it only by id, such as a staff invite's; none
    /// for an id no tenant has.
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn name(&self, id: i64) -> Result<Option<String>, Error> {
        self.store.find_name(id).await
    }

    /// Moves the tenant at an address to a new slug and its address, such as one whose address a release has
    /// since reserved for Tetsu's own pages (#90). The address may be reserved; the new slug may not, since it
    /// passed [`Slug::parse`].
    ///
    /// # Errors
    /// Fails with [`Error::NotFound`] when no tenant is at the address, [`Error::SlugTaken`] or
    /// [`Error::AddressTaken`] when another tenant holds the new slug or its address, or if storage fails.
    pub async fn rename(&self, address: &str, to: &Slug) -> Result<(), Error> {
        let address = normalized(address).ok_or(Error::NotFound)?;
        if self.store.rename(&address, to).await? {
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    /// The stored addresses that a name reserved for Tetsu's own pages now claims. A site route always wins
    /// over a tenant's address, so the tenant at one would be hidden without a word; the composition root
    /// refuses to serve while any is held (#90).
    ///
    /// # Errors
    /// Fails if storage fails.
    pub async fn reserved_addresses_in_use(&self) -> Result<Vec<String>, Error> {
        self.store
            .find_addresses_in_use(&crate::domain::reserved_addresses())
            .await
    }
}

/// An address as stored: lowercase, with ASCII whitespace trimmed, since no address holds any. One holding a
/// control character is no address at all; other whitespace, such as U+2028, stays and makes it unknown.
fn normalized(address: &str) -> Option<String> {
    if address.chars().any(char::is_control) {
        return None;
    }
    Some(
        address
            .trim_matches(|c: char| c.is_ascii_whitespace())
            .to_ascii_lowercase(),
    )
}
