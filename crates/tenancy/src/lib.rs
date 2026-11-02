//! Tenancy: who a piece of data belongs to.
//!
//! Every platform capability that reads or writes tenant data takes a
//! [`TenantContext`]. There is no way to build one without a [`TenantId`], so
//! "forgot to scope the query" becomes a compile error instead of a data leak.
//! Postgres row-level security enforces the same rule a second time in the
//! database (ADR-0002, `docs/adr/0002-tenant-isolation.md`).

use std::fmt;
use std::str::FromStr;

/// Identifies one tenant (for the fitness vertical, one studio brand).
///
/// A tenant ID is a version 7 UUID. Tetsu never mints IDs itself: Postgres 18's
/// `uuidv7()` generates them when a tenant row is inserted (ADR-0002). This
/// type only carries, formats and parses them. A tenant ID is an identifier,
/// not a secret: knowing one never grants access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(u128);

impl TenantId {
    /// Wraps the 128-bit value of a UUID, for example one read from Postgres.
    #[must_use]
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }

    /// Returns the 128-bit value.
    #[must_use]
    pub const fn as_u128(&self) -> u128 {
        self.0
    }
}

impl fmt::Display for TenantId {
    /// Formats as a lowercase, hyphenated UUID, e.g.
    /// `0192f1c4-7d2a-7c3e-9a1b-5c6d7e8f9a0b`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.0;
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            v >> 96,
            (v >> 80) & 0xffff,
            (v >> 64) & 0xffff,
            (v >> 48) & 0xffff,
            v & 0xffff_ffff_ffff
        )
    }
}

/// Returned when a string is not a canonical hyphenated UUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTenantIdError;

impl fmt::Display for ParseTenantIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid tenant ID: expected a hyphenated UUID")
    }
}

impl std::error::Error for ParseTenantIdError {}

impl FromStr for TenantId {
    type Err = ParseTenantIdError;

    /// Accepts the canonical 36-character form, in upper or lower case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const HYPHENS: [usize; 4] = [8, 13, 18, 23];
        let bytes = s.as_bytes();
        if bytes.len() != 36 || HYPHENS.iter().any(|&i| bytes[i] != b'-') {
            return Err(ParseTenantIdError);
        }
        let hex: String = s.chars().filter(|&c| c != '-').collect();
        if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ParseTenantIdError);
        }
        u128::from_str_radix(&hex, 16)
            .map(Self)
            .map_err(|_| ParseTenantIdError)
    }
}

/// Proof that the current operation is scoped to exactly one tenant.
///
/// Inbound adapters (HTTP, jobs) build this once, after resolving and
/// authorizing the tenant, and pass it down. Domain code never resolves
/// tenants itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantContext {
    tenant_id: TenantId,
}

impl TenantContext {
    /// Scopes an operation to `tenant_id`.
    #[must_use]
    pub const fn new(tenant_id: TenantId) -> Self {
        Self { tenant_id }
    }

    /// The tenant this operation is scoped to.
    #[must_use]
    pub const fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "0192f1c4-7d2a-7c3e-9a1b-5c6d7e8f9a0b";

    #[test]
    fn display_and_parse_round_trip() {
        let id: TenantId = SAMPLE.parse().expect("valid UUID");
        assert_eq!(id.to_string(), SAMPLE);
    }

    #[test]
    fn formats_leading_zeros() {
        assert_eq!(
            TenantId::from_u128(1).to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
    }

    #[test]
    fn parses_uppercase() {
        let upper = SAMPLE.to_uppercase();
        assert_eq!(
            upper.parse::<TenantId>().map(|id| id.to_string()),
            Ok(SAMPLE.to_owned())
        );
    }

    #[test]
    fn rejects_malformed_ids() {
        for bad in [
            "",
            "studio-42",
            "0192f1c47d2a7c3e9a1b5c6d7e8f9a0b",     // no hyphens
            "0192f1c4-7d2a-7c3e-9a1b-5c6d7e8f9a0",  // too short
            "0192f1c4-7d2a-7c3e-9a1b-5c6d7e8f9a0g", // not hex
            "0192f1c47-d2a-7c3e-9a1b-5c6d7e8f9a0b", // hyphen in wrong place
        ] {
            assert_eq!(bad.parse::<TenantId>(), Err(ParseTenantIdError), "{bad:?}");
        }
    }

    #[test]
    fn context_carries_its_tenant() {
        let id = TenantId::from_u128(42);
        assert_eq!(TenantContext::new(id).tenant_id(), id);
    }
}
