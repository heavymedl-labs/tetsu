//! What Mail decides by: who gets real email, and what a kept message shows.

use tetsu_kernel::Address;

/// The addresses that get real email by SMTP (ADR-0044). Every other message is kept for the mailbox page, so the
/// demo businesses' fictional people, and anyone not named here, never receive anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowlist(Vec<Address>);

impl Allowlist {
    /// The addresses in `text`, separated by commas, or none if any entry is not a plain address. Spaces around an
    /// entry are ignored, and empty text names nobody.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.trim().is_empty() {
            return Some(Self::default());
        }
        text.split(',')
            .map(|entry| Address::parse(entry.trim()))
            .collect::<Option<_>>()
            .map(Self)
    }

    /// Whether `address` is on the list. A local part is compared exactly, since only its own server may say two
    /// spellings are the same mailbox.
    #[must_use]
    pub fn contains(&self, address: &Address) -> bool {
        self.0.contains(address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(text: &str) -> Address {
        Address::parse(text).unwrap()
    }

    #[test]
    fn an_allowlist_names_each_address_it_lists() {
        let list = Allowlist::parse(" ada@example.com,lee@Example.ORG ").unwrap();
        assert!(list.contains(&address("ada@example.com")));
        assert!(list.contains(&address("lee@example.org")));
        assert!(!list.contains(&address("Ada@example.com")));
        assert!(!list.contains(&address("eve@example.com")));
    }

    #[test]
    fn an_empty_allowlist_names_nobody() {
        for text in ["", "  "] {
            let list = Allowlist::parse(text).unwrap();
            assert_eq!(list, Allowlist::default());
            assert!(!list.contains(&address("ada@example.com")));
        }
    }

    #[test]
    fn an_allowlist_with_any_entry_that_is_not_plain_is_refused() {
        for text in [
            "ada@example.com,",
            "ada@example.com, , lee@example.org",
            "Ada <ada@example.com>",
            "ada@example.com;lee@example.org",
        ] {
            assert_eq!(Allowlist::parse(text), None, "{text:?}");
        }
    }
}
