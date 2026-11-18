//! The `Mailer` port (ADR-0044, ADR-0074): what a module hands over to send an email, and what came of it. The Mail
//! module implements it, and the binary hands it to the modules that send.

use std::fmt;
use std::future::Future;

/// Longest address a mail server accepts in a recipient command (RFC 5321), which also bounds the domain.
const MAX_ADDRESS: usize = 254;
const MAX_LOCAL: usize = 64;
const MAX_LABEL: usize = 63;
/// Longest idempotency key, as the Mail module stores it.
const MAX_KEY: usize = 128;

/// A plain email address: a dot-atom local part, `@`, and a domain of letters, digits, and hyphens with at least
/// two labels, written in lowercase. Anything else is refused, such as a quoted local part, a display name, an
/// internationalized domain, or a line break, so no address can carry more than one recipient or a header
/// (ADR-0074).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Address {
    text: String,
    at: usize,
}

impl Address {
    /// The address `text` names, or none if it is not plain.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (local, domain) = text.split_once('@')?;
        let plain = text.len() <= MAX_ADDRESS && local_part(local) && domain_part(domain);
        plain.then(|| Self {
            text: format!("{local}@{}", domain.to_ascii_lowercase()),
            at: local.len(),
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The part before the `@`.
    #[must_use]
    pub fn local(&self) -> &str {
        &self.text[..self.at]
    }

    /// The part after the `@`, in lowercase.
    #[must_use]
    pub fn domain(&self) -> &str {
        &self.text[self.at + 1..]
    }
}

/// An address is personal data, so its debug form does not show it, and a log of a value holding one cannot leak
/// it.
impl fmt::Debug for Address {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Address(..)")
    }
}

fn local_part(local: &str) -> bool {
    (1..=MAX_LOCAL).contains(&local.len())
        && local.split('.').all(|atom| {
            !atom.is_empty()
                && atom
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+/=?^_`{|}~-".contains(&b))
        })
}

fn domain_part(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            (1..=MAX_LABEL).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// One email to send. Its key names the event that caused it, so it goes out once however often it is handed over
/// (ADR-0044).
#[derive(Clone, PartialEq, Eq)]
pub struct Message {
    to: Address,
    subject: String,
    text: String,
    html: String,
    key: String,
}

impl Message {
    /// A message, or none if its subject holds a line break, or its key is empty, longer than 128 bytes, or holds
    /// anything but letters, digits, `.`, `_`, `:`, and `-`.
    #[must_use]
    pub fn new(to: Address, subject: &str, text: &str, html: &str, key: &str) -> Option<Self> {
        let subject_ok = !subject.contains(['\r', '\n']);
        let key_ok = (1..=MAX_KEY).contains(&key.len())
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b));
        (subject_ok && key_ok).then(|| Self {
            to,
            subject: subject.to_string(),
            text: text.to_string(),
            html: html.to_string(),
            key: key.to_string(),
        })
    }

    #[must_use]
    pub fn to(&self) -> &Address {
        &self.to
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn html(&self) -> &str {
        &self.html
    }

    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// A message may hold a live link, so its debug form shows only its key.
impl fmt::Debug for Message {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Message")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// What came of handing a message over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// A mail server took it for its recipient.
    Sent,
    /// It was kept for the mailbox page rather than sent, since its recipient is not on the allowlist.
    Captured,
    /// Its key was handed over before, so nothing more was done.
    Repeated,
}

/// Why a message did not go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailError {
    /// The mail server or the store could not be reached or failed; trying again later may work.
    Unavailable,
    /// The mail server refused the message for good, such as an address it will not deliver to; trying again will
    /// not help.
    Refused,
}

/// Sends email for the modules (ADR-0044). A message goes out once per key: handing the same key over again does
/// nothing more, and a failed attempt leaves the key free to try again.
pub trait Mailer: Send + Sync {
    /// Sends or captures `message`, as its recipient and the allowlist decide.
    fn send(&self, message: &Message) -> impl Future<Output = Result<Delivery, MailError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_addresses_parse_with_their_domain_in_lowercase() {
        for (text, address) in [
            ("ada@example.com", "ada@example.com"),
            (
                "Ada.Lovelace+ops@Example.COM",
                "Ada.Lovelace+ops@example.com",
            ),
            (
                "a!#$%&'*/=?^_`{|}~-z@mail.example-1.org",
                "a!#$%&'*/=?^_`{|}~-z@mail.example-1.org",
            ),
            ("x@a.bc", "x@a.bc"),
        ] {
            assert_eq!(
                Address::parse(text).map(|a| a.text),
                Some(address.to_string()),
                "{text}"
            );
        }
    }

    #[test]
    fn anything_but_a_plain_address_is_refused() {
        let long_local = format!("{}@example.com", "a".repeat(65));
        let long_label = format!("a@{}.com", "b".repeat(64));
        let long_domain = format!("a@{}.com", vec!["c".repeat(60); 5].join("."));
        for text in [
            "",
            "ada",
            "@example.com",
            "ada@",
            "ada@localhost",
            "\"ada\"@example.com",
            "\"a b\"@example.com",
            "Ada <ada@example.com>",
            "ada@example.com\r\nBcc: eve@example.com",
            "ada\n@example.com",
            "ada@exa mple.com",
            "a..da@example.com",
            ".ada@example.com",
            "ada.@example.com",
            "ada@@example.com",
            "ada@example..com",
            "ada@-example.com",
            "ada@example-.com",
            "ada@exam_ple.com",
            "ada@example.com.",
            "ada@[192.0.2.1]",
            "adä@example.com",
            "ada@exämple.com",
            "a(b)@example.com",
            &long_local,
            &long_label,
            &long_domain,
        ] {
            assert!(Address::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn limits_hold_at_their_edges() {
        let local = format!("{}@example.com", "a".repeat(64));
        let label = format!("a@{}.com", "b".repeat(63));
        assert!(Address::parse(&local).is_some());
        assert!(Address::parse(&label).is_some());
        let address = format!(
            "{}@{}",
            "a".repeat(64),
            ["e".repeat(63), "f".repeat(62), "g".repeat(62)].join(".")
        );
        assert_eq!(address.len(), 254);
        assert!(Address::parse(&address).is_some());
        let over = format!(
            "{}@{}",
            "a".repeat(64),
            ["e".repeat(63), "f".repeat(62), "g".repeat(63)].join(".")
        );
        assert_eq!(over.len(), 255);
        assert!(Address::parse(&over).is_none());
    }

    #[test]
    fn an_address_and_a_message_keep_personal_data_out_of_debug() {
        let address = Address::parse("ada@example.com").unwrap();
        assert_eq!(format!("{address:?}"), "Address(..)");
        assert_eq!(address.as_str(), "ada@example.com");
        assert_eq!((address.local(), address.domain()), ("ada", "example.com"));
        let tagged = Address::parse("Ada+ops@Mail.Example.COM").unwrap();
        assert_eq!(
            (tagged.local(), tagged.domain()),
            ("Ada+ops", "mail.example.com")
        );
        let message = Message::new(
            address,
            "Hi",
            "secret link",
            "<p>secret link</p>",
            "invite:abc",
        )
        .unwrap();
        assert_eq!(
            format!("{message:?}"),
            "Message { key: \"invite:abc\", .. }"
        );
    }

    #[test]
    fn a_message_keeps_what_it_was_given() {
        let to = Address::parse("ada@example.com").unwrap();
        let message = Message::new(
            to.clone(),
            "Your invite",
            "text",
            "<p>html</p>",
            "a.b_c:d-1",
        )
        .unwrap();
        assert_eq!(message.to(), &to);
        assert_eq!(message.subject(), "Your invite");
        assert_eq!(message.text(), "text");
        assert_eq!(message.html(), "<p>html</p>");
        assert_eq!(message.key(), "a.b_c:d-1");
    }

    #[test]
    fn a_message_refuses_a_broken_subject_or_key() {
        let to = || Address::parse("ada@example.com").unwrap();
        for (subject, key) in [
            ("Hi\r\nBcc: eve@example.com", "k"),
            ("Hi\nthere", "k"),
            ("Hi\r", "k"),
            ("Hi", ""),
            ("Hi", "has space"),
            ("Hi", "slash/no"),
            ("Hi", "ü"),
        ] {
            assert!(
                Message::new(to(), subject, "t", "h", key).is_none(),
                "{subject:?} {key:?}"
            );
        }
        assert!(Message::new(to(), "Hi", "t", "h", &"k".repeat(128)).is_some());
        assert!(Message::new(to(), "Hi", "t", "h", &"k".repeat(129)).is_none());
    }
}
