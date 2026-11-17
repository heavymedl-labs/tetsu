//! The contract every Tetsu module implements (ADR-0003).
//!
//! This is a contract, not a framework: modules plug in at compile time, and
//! the `tetsu` binary registers them.

use std::net::IpAddr;

/// One numbered SQL file. Versions only ever grow, and an applied file never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub version: i32,
    pub sql: &'static str,
}

/// Where a module keeps its data: the one Postgres schema it owns, and its migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Storage {
    pub schema: &'static str,
    pub migrations: &'static [Migration],
}

/// A module that plugs into the kernel.
pub trait Module {
    /// The module's name, unique among the modules the binary registers.
    fn name(&self) -> &'static str;

    /// The module's storage, if it stores data.
    fn storage(&self) -> Option<Storage> {
        None
    }
}

/// The color scheme a visitor chose for Tetsu's pages, kept in the `theme` cookie (ADR-0070): light or dark, or the
/// system's, which is no cookie. Every module's pages share one page shell, which renders the choice, so they read it
/// the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    #[default]
    System,
    Light,
    Dark,
}

impl Scheme {
    /// The scheme the `theme` cookie names among a request's `Cookie` headers; none, or any other value, is the
    /// system's.
    #[must_use]
    pub fn from_cookies<'a>(headers: impl IntoIterator<Item = &'a str>) -> Self {
        headers
            .into_iter()
            .flat_map(|header| header.split(';'))
            .find_map(|pair| match pair.trim() {
                "theme=light" => Some(Self::Light),
                "theme=dark" => Some(Self::Dark),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The radio the page checks for it, as the page shell names them.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// The network a request came from, as a rate limit keys on it (ADR-0072): an IPv4 address, mapped or not, as
/// itself, and an IPv6 address as its /64, since one home or phone holds a whole /64 and can move within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requester([u8; 16]);

impl Requester {
    /// The network `address` is in.
    #[must_use]
    pub fn of(address: IpAddr) -> Self {
        match address.to_canonical() {
            IpAddr::V4(v4) => Self(v4.to_ipv6_mapped().octets()),
            IpAddr::V6(v6) => {
                let mut prefix = [0; 16];
                prefix[..8].copy_from_slice(&v6.octets()[..8]);
                Self(prefix)
            }
        }
    }

    /// The network a request came from, given its connection's address and its `X-Forwarded-For` headers in
    /// order: with `proxies` in front of Tetsu, each adding the address it was sent from, the entry that many from
    /// the right, which the first of them added. Entries left of it are whatever the client sent. With no
    /// proxies, a header with too few entries, or an entry that is not an address, the connection's own.
    #[must_use]
    pub fn from_request<'a>(
        connection: IpAddr,
        forwarded: impl IntoIterator<Item = &'a str>,
        proxies: usize,
    ) -> Self {
        let entries: Vec<&str> = forwarded
            .into_iter()
            .flat_map(|header| header.split(','))
            .collect();
        let added = entries
            .len()
            .checked_sub(proxies)
            .filter(|_| proxies > 0)
            .and_then(|at| entries[at].trim().parse().ok());
        Self::of(added.unwrap_or(connection))
    }

    /// The network as 16 bytes, an IPv4 address mapped into IPv6, for a store to key on.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    /// IPv4 keys on the whole address, mapped or not; IPv6 on its /64, so moving within it changes nothing.
    #[test]
    fn a_requester_is_an_ipv4_address_or_an_ipv6_slash_64() {
        assert_eq!(
            Requester::of(ip("192.0.2.1")),
            Requester::of(ip("::ffff:192.0.2.1"))
        );
        assert_eq!(
            Requester::of(ip("192.0.2.1")).as_bytes(),
            &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 0, 2, 1]
        );
        assert_ne!(
            Requester::of(ip("192.0.2.1")),
            Requester::of(ip("192.0.2.2"))
        );
        assert_eq!(
            Requester::of(ip("2001:db8:1:2:aaaa::1")),
            Requester::of(ip("2001:db8:1:2:bbbb::9"))
        );
        assert_eq!(
            Requester::of(ip("2001:db8:1:2:aaaa::1")).as_bytes(),
            &[0x20, 0x01, 0x0d, 0xb8, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_ne!(
            Requester::of(ip("2001:db8:1:2::1")),
            Requester::of(ip("2001:db8:1:3::1"))
        );
    }

    /// Behind three proxies, as on Render, the requester is the third entry from the right, across every header,
    /// whatever the client put before it; anything else falls back to the connection.
    #[test]
    fn the_requester_is_the_entry_the_first_trusted_proxy_added() {
        let connection = ip("10.0.0.9");
        let forged = ["203.0.113.7, 198.51.100.9,192.0.2.1, 162.159.1.1, 10.1.1.1"];
        for (forwarded, proxies, requester) in [
            (&forged[..], 3, "192.0.2.1"),
            (
                &["203.0.113.7", "192.0.2.1, 162.159.1.1", " 10.1.1.1 "][..],
                3,
                "192.0.2.1",
            ),
            (&["2001:db8::5, 162.159.1.1, 10.1.1.1"][..], 3, "2001:db8::"),
            (&forged[..], 0, "10.0.0.9"),
            (&forged[..], 1, "10.1.1.1"),
            (&["162.159.1.1, 10.1.1.1"][..], 3, "10.0.0.9"),
            (&["unknown, 162.159.1.1, 10.1.1.1"][..], 3, "10.0.0.9"),
            (&[][..], 3, "10.0.0.9"),
        ] {
            assert_eq!(
                Requester::from_request(connection, forwarded.iter().copied(), proxies),
                Requester::of(ip(requester)),
                "{forwarded:?} behind {proxies}"
            );
        }
    }

    #[test]
    fn a_scheme_is_read_from_the_theme_cookie() {
        for (headers, scheme) in [
            (vec!["theme=dark"], Scheme::Dark),
            (vec!["a=1; theme=light; b=2"], Scheme::Light),
            (vec!["a=1", "theme=dark"], Scheme::Dark),
            (vec![" theme=dark "], Scheme::Dark),
        ] {
            assert_eq!(Scheme::from_cookies(headers.clone()), scheme, "{headers:?}");
        }
    }

    #[test]
    fn a_missing_or_unknown_theme_is_the_system_scheme() {
        for headers in [
            vec![],
            vec!["theme="],
            vec!["theme=Dark"],
            vec!["theme=system"],
            vec!["xtheme=dark"],
            vec!["theme=dark2"],
        ] {
            assert_eq!(
                Scheme::from_cookies(headers.clone()),
                Scheme::System,
                "{headers:?}"
            );
        }
    }

    #[test]
    fn a_scheme_names_the_radio_it_checks() {
        assert_eq!(
            [Scheme::System, Scheme::Light, Scheme::Dark].map(Scheme::as_str),
            ["system", "light", "dark"]
        );
        assert_eq!(Scheme::default(), Scheme::System);
    }

    struct Plain;

    impl Module for Plain {
        fn name(&self) -> &'static str {
            "plain"
        }
    }

    #[test]
    fn a_module_stores_nothing_unless_it_says_so() {
        assert_eq!(Plain.name(), "plain");
        assert_eq!(Plain.storage(), None);
    }
}
