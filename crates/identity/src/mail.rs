//! The email Identity sends (ADR-0074), rendered from templates, so no markup lives in Rust.

use askama::Template;
use tetsu_kernel::{Address, Message};

use crate::domain::{InviteToken, Role, to_hex};

const INVITE_SUBJECT: &str = "Your invite to Tetsu";

#[derive(Template)]
#[template(path = "mail/invite.txt")]
struct InviteText<'a> {
    link: &'a str,
}

#[derive(Template)]
#[template(path = "mail/invite.html")]
struct InviteHtml<'a> {
    link: &'a str,
}

/// An admin's invite to `to`, carrying `link`, keyed on the link's token, so the same link is sent once
/// however often it is handed over. The key holds the token's hash, never the token, and the invite is never kept
/// for the mailbox, where another admin could read the live link (ADR-0074).
#[must_use]
pub fn invite_message(to: Address, link: &str, token: &InviteToken) -> Option<Message> {
    invite(to, token, &InviteText { link }, &InviteHtml { link })
}

/// An invite to `to` in two bodies, keyed on its link's token and never kept.
fn invite(
    to: Address,
    token: &InviteToken,
    text: &impl Template,
    html: &impl Template,
) -> Option<Message> {
    let key = format!("invite:{}", to_hex(&token.hash()));
    text.render()
        .ok()
        .zip(html.render().ok())
        .and_then(|(text, html)| Message::new(to, INVITE_SUBJECT, &text, &html, &key))
        .map(Message::never_kept)
}

#[derive(Template)]
#[template(path = "mail/staff_invite.txt")]
struct StaffInviteText<'a> {
    business: &'a str,
    role: &'a str,
    link: &'a str,
}

#[derive(Template)]
#[template(path = "mail/staff_invite.html")]
struct StaffInviteHtml<'a> {
    business: &'a str,
    role: &'a str,
    link: &'a str,
}

/// A staff member's invite to `business` as `role`, carrying `link`, keyed and never kept as an admin's invite is,
/// so only the invited address ever holds the link (ADR-0074, ADR-0089).
#[must_use]
pub fn staff_invite_message(
    to: Address,
    business: &str,
    role: Role,
    link: &str,
    token: &InviteToken,
) -> Option<Message> {
    let role = match role {
        Role::Owner => "its owner",
    };
    invite(
        to,
        token,
        &StaffInviteText {
            business,
            role,
            link,
        },
        &StaffInviteHtml {
            business,
            role,
            link,
        },
    )
}

/// What a notice tells an admin of, by the event that records it (ADR-0074).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Notice {
    Login,
    Passkey,
}

impl Notice {
    /// The notice an event of `kind` calls for, if any.
    pub(crate) fn of(kind: &str) -> Option<Self> {
        match kind {
            "identity.logged_in" => Some(Self::Login),
            // Joining stores the admin's first passkey, so it is told like any other.
            "identity.passkey_added" | "identity.invite_redeemed" => Some(Self::Passkey),
            _ => None,
        }
    }

    fn subject(self) -> &'static str {
        match self {
            Self::Login => "New login to your Tetsu account",
            Self::Passkey => "A passkey was added to your Tetsu account",
        }
    }

    fn what(self) -> &'static str {
        match self {
            Self::Login => "You logged in to Tetsu",
            Self::Passkey => "A passkey was added to your Tetsu account",
        }
    }
}

#[derive(Template)]
#[template(path = "mail/notice.txt")]
struct NoticeText<'a> {
    what: &'a str,
    at: &'a str,
    device: &'a str,
}

#[derive(Template)]
#[template(path = "mail/notice.html")]
struct NoticeHtml<'a> {
    subject: &'a str,
    what: &'a str,
    at: &'a str,
    device: &'a str,
}

/// When and from where a notice's event happened: `at` in words, and the coarse label of the device that acted
/// (`v5.0.0-6.3.5`).
#[derive(Clone, Copy)]
pub(crate) struct Happened<'a> {
    pub(crate) at: &'a str,
    pub(crate) device: &'a str,
}

/// A notice to `email` of what `happened`, keyed on the event that recorded it, so a retried event is told once;
/// none for an address that is not plain.
pub(crate) fn notice_message(
    notice: Notice,
    email: &str,
    happened: &Happened<'_>,
    event: &str,
) -> Option<Message> {
    let (subject, what) = (notice.subject(), notice.what());
    let Happened { at, device } = *happened;
    Address::parse(email)
        .zip(NoticeText { what, at, device }.render().ok())
        .zip(
            NoticeHtml {
                subject,
                what,
                at,
                device,
            }
            .render()
            .ok(),
        )
        .and_then(|((to, text), html)| {
            Message::new(to, subject, &text, &html, &format!("notice:{event}"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str) -> InviteToken {
        InviteToken::parse(text).unwrap()
    }

    #[test]
    fn an_invite_carries_its_link_in_both_bodies() {
        let link = "https://tetsu.example/admin/join#abc&def";
        let token = token(&"ab".repeat(32));
        let message =
            invite_message(Address::parse("ada@example.com").unwrap(), link, &token).unwrap();
        assert_eq!(message.to().as_str(), "ada@example.com");
        assert_eq!(message.subject(), "Your invite to Tetsu");
        assert!(!message.may_be_kept());
        assert!(
            message
                .text()
                .contains("\nhttps://tetsu.example/admin/join#abc&def\n"),
        );
        assert!(
            message
                .html()
                .contains(r#"<a href="https://tetsu.example/admin/join#abc&#38;def">"#),
        );
        assert!(message.text().contains("works once, for an hour"));
    }

    #[test]
    fn a_staff_invite_names_its_business_and_role_as_escaped_text() {
        let link = "https://tetsu.example/app/join#abc&def";
        let token = token(&"ab".repeat(32));
        let business = "Fern & Co <a href=\"https://evil.example\">";
        let message = staff_invite_message(
            Address::parse("ada@example.com").unwrap(),
            business,
            Role::Owner,
            link,
            &token,
        )
        .unwrap();
        assert_eq!(message.to().as_str(), "ada@example.com");
        assert_eq!(message.subject(), "Your invite to Tetsu");
        assert!(!message.may_be_kept());
        assert_eq!(message.key(), format!("invite:{}", to_hex(&token.hash())));
        assert!(
            message
                .text()
                .contains(&format!("work at {business} on Tetsu, as its owner"))
        );
        assert!(
            message
                .text()
                .contains("\nhttps://tetsu.example/app/join#abc&def\n")
        );
        assert!(message.text().contains("works once, for seven days"));
        assert!(
            message.html().contains(
                "work at Fern &#38; Co &#60;a href=&#34;https://evil.example&#34;&#62; on"
            )
        );
        assert_eq!(message.html().matches("<a ").count(), 1);
        assert!(
            message
                .html()
                .contains(r#"<a href="https://tetsu.example/app/join#abc&#38;def">"#),
        );
    }

    #[test]
    fn a_notice_says_what_happened_and_when() {
        assert_eq!(Notice::of("identity.logged_in"), Some(Notice::Login));
        assert_eq!(Notice::of("identity.passkey_added"), Some(Notice::Passkey));
        assert_eq!(
            Notice::of("identity.invite_redeemed"),
            Some(Notice::Passkey)
        );
        assert_eq!(Notice::of("identity.joined"), None);
        let at = &Happened {
            at: "November 18, 2026, 21:04 UTC",
            device: "Firefox on Linux",
        };
        let login = notice_message(Notice::Login, "ada@example.com", at, "e-1").unwrap();
        assert_eq!(login.subject(), "New login to your Tetsu account");
        assert_eq!(login.key(), "notice:e-1");
        assert!(login.text().contains(
            "You logged in to Tetsu on November 18, 2026, 21:04 UTC, from Firefox on Linux."
        ));
        assert!(login.html().contains(
            "<p>You logged in to Tetsu on November 18, 2026, 21:04 UTC, from Firefox on Linux.</p>"
        ));
        assert!(!login.html().contains("href"));
        let passkey = notice_message(Notice::Passkey, "ada@example.com", at, "e-2").unwrap();
        assert_eq!(
            passkey.subject(),
            "A passkey was added to your Tetsu account"
        );
        assert!(
            passkey
                .text()
                .contains("A passkey was added to your Tetsu account on ")
        );
        assert!(notice_message(Notice::Login, "ada@localhost", at, "e-3").is_none());
    }

    #[test]
    fn an_invite_is_keyed_on_its_token_without_holding_it() {
        let to = || Address::parse("ada@example.com").unwrap();
        let text = "ab".repeat(32);
        let first = invite_message(to(), "l", &token(&text)).unwrap();
        let again = invite_message(to(), "other", &token(&text)).unwrap();
        let other = invite_message(to(), "l", &token(&"cd".repeat(32))).unwrap();
        assert_eq!(first.key(), again.key());
        assert_ne!(first.key(), other.key());
        assert!(first.key().starts_with("invite:"));
        assert_eq!(first.key().len(), "invite:".len() + 64);
        assert!(!first.key().contains(&text[..16]));
        assert_eq!(
            first.key(),
            format!("invite:{}", to_hex(&token(&text).hash()))
        );
    }
}
