//! The email Identity sends (ADR-0074), rendered from templates, so no markup lives in Rust.

use askama::Template;
use tetsu_kernel::{Address, Message};

use crate::domain::{InviteToken, to_hex};

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

/// An operator's invite to `to`, carrying `link`, keyed on the link's token, so the same link is sent once
/// however often it is handed over. The key holds the token's hash, never the token.
#[must_use]
pub fn invite_message(to: Address, link: &str, token: &InviteToken) -> Option<Message> {
    let key = format!("invite:{}", to_hex(&token.hash()));
    InviteText { link }
        .render()
        .ok()
        .zip(InviteHtml { link }.render().ok())
        .and_then(|(text, html)| Message::new(to, INVITE_SUBJECT, &text, &html, &key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str) -> InviteToken {
        InviteToken::parse(text).unwrap()
    }

    #[test]
    fn an_invite_carries_its_link_in_both_bodies() {
        let link = "https://tetsu.example/ops/join#abc&def";
        let token = token(&"ab".repeat(32));
        let message =
            invite_message(Address::parse("ada@example.com").unwrap(), link, &token).unwrap();
        assert_eq!(message.to().as_str(), "ada@example.com");
        assert_eq!(message.subject(), "Your invite to Tetsu");
        assert!(
            message
                .text()
                .contains("\nhttps://tetsu.example/ops/join#abc&def\n"),
        );
        assert!(
            message
                .html()
                .contains(r#"<a href="https://tetsu.example/ops/join#abc&#38;def">"#),
        );
        assert!(message.text().contains("works once, for an hour"));
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
