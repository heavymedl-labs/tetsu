//! The SMTP adapter (ADR-0044, ADR-0074): one message to one recipient over implicit TLS, never STARTTLS, through
//! `lettre`. `lettre`'s types stay in this file (ADR-0007).

use std::fmt::Write as _;
use std::future::{Future, ready};
use std::pin::Pin;
use std::time::Duration;

use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::Error as SmtpError;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Certificate, Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use tetsu_kernel::{Address, Message};

/// Where and as whom Tetsu sends: the provider's host and implicit-TLS port, its login, and the address mail comes
/// from. The password is a secret (ADR-0074), so this has no debug form.
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: Address,
    /// How long one whole send may take, connecting included, before it counts as failed. It stays well under the
    /// two minutes after which a claim lapses, or a slow send could go twice.
    pub limit: Duration,
}

/// Sends over SMTP with implicit TLS, trusting the web's public certificate authorities.
pub struct Smtp {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    limit: Duration,
}

/// Names what is wrong without repeating the host, which a mistaken setting could fill with a login.
const REFUSED_HOST: &str = "the SMTP host is not a name TLS can check";

type Sending<'a> = Pin<Box<dyn Future<Output = Result<(), Failed>> + Send + 'a>>;

impl Smtp {
    /// An adapter for `settings`.
    ///
    /// # Errors
    /// Fails if the host is not a name TLS can check a certificate against, which `lettre` would only find when it
    /// first connects, so a wrong setting stops Tetsu at start rather than at its first email.
    pub fn new(settings: &SmtpSettings) -> Result<Self, String> {
        let tls = ServerName::try_from(settings.host.as_str())
            .ok()
            .and_then(|_| TlsParameters::new(settings.host.clone()).ok());
        Ok(Self::with_tls(settings, tls.ok_or(REFUSED_HOST)?))
    }

    /// An adapter that also trusts the certificate authority in `pem`, as a test's local mail server needs.
    ///
    /// # Errors
    /// Fails if `pem` holds no certificate, or as [`Self::new`].
    #[doc(hidden)]
    pub fn trusting(settings: &SmtpSettings, pem: &[u8]) -> Result<Self, String> {
        let tls = ServerName::try_from(settings.host.as_str())
            .ok()
            .and(CertificateDer::from_pem_slice(pem).ok())
            .and_then(|_| Certificate::from_pem(pem).ok())
            .and_then(|authority| {
                TlsParameters::builder(settings.host.clone())
                    .add_root_certificate(authority)
                    .build_rustls()
                    .ok()
            });
        let refused = format!("{REFUSED_HOST}, or the PEM holds no certificate");
        Ok(Self::with_tls(settings, tls.ok_or(refused)?))
    }

    /// `lettre` refuses TLS settings only for options Tetsu never sets, such as an old minimum TLS version, so a
    /// refusal there counts as the host being refused.
    fn with_tls(settings: &SmtpSettings, tls: TlsParameters) -> Self {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&settings.host)
            .port(settings.port)
            .tls(Tls::Wrapper(tls))
            .credentials(Credentials::new(
                settings.username.clone(),
                settings.password.clone(),
            ))
            .timeout(Some(settings.limit))
            .build();
        Self {
            transport,
            from: mailbox(&settings.from),
            limit: settings.limit,
        }
    }

    /// Sends `message` as a plain-text and HTML alternative.
    ///
    /// # Errors
    /// Why it did not go, as [`Failed`] sorts it. A send past the limit is `Unknown`.
    pub(crate) async fn deliver(&self, message: &Message) -> Result<(), Failed> {
        // The key in hex: a valid `Message-ID` for any key, and a different one for each.
        let key = message.key().bytes().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        });
        let id = format!("<{key}@{}>", self.from.email.domain());
        let email = lettre::Message::builder()
            .message_id(Some(id))
            .from(self.from.clone())
            .to(mailbox(message.to()))
            .subject(message.subject())
            .multipart(MultiPart::alternative_plain_html(
                message.text().to_string(),
                message.html().to_string(),
            ));
        // `lettre` refuses a message only without a sender or a recipient, which this one always has, so the
        // refusal is chosen without a branch no test could reach.
        let refusal: Sending<'_> = Box::pin(ready(Err(Failed::Refused)));
        let sending = email.ok().map_or(refusal, |email| -> Sending<'_> {
            Box::pin(async move {
                self.transport
                    .send(email)
                    .await
                    .map(drop)
                    .map_err(|error| sort(&error))
            })
        });
        tokio::time::timeout(self.limit, sending)
            .await
            .unwrap_or(Err(Failed::Unknown))
    }
}

/// The mailbox for a plain address, with no display name. A plain address is stricter than any `lettre` checks,
/// so it is built without checking again.
fn mailbox(address: &Address) -> Mailbox {
    Mailbox::new(
        None,
        lettre::Address::new_dangerous(address.local(), address.domain()),
    )
}

/// Why a send did not go, which decides what becomes of its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failed {
    /// The recipient's mailbox was refused for good.
    Refused,
    /// The server answered with an error, so it did not take the message: try again at once.
    NotTaken,
    /// Whether the server took it is not known, as after a timeout, a failed TLS handshake, or a dropped connection:
    /// try again only once the claim lapses, so a message it did take goes at most twice.
    Unknown,
}

/// Only a refused mailbox is final. Every other failure, such as a refused login or sender, a quota, a TLS failure,
/// or an outage, is Tetsu's setup or the server's state, which a fix mends. Each is logged by its reply code and
/// whether it timed out, failed TLS, or was permanent, which hold no address and no secret.
fn sort(error: &SmtpError) -> Failed {
    let code = error.status().map(|code| code.to_string());
    let (timeout, tls, permanent) = (error.is_timeout(), error.is_tls(), error.is_permanent());
    tracing::warn!(
        code,
        timeout,
        tls,
        permanent,
        "mail: the SMTP server did not take a message"
    );
    if mailbox_refused(code.as_deref(), &error.to_string()) {
        Failed::Refused
    } else if code.is_some() {
        Failed::NotTaken
    } else {
        Failed::Unknown
    }
}

/// Whether a reply refuses the recipient's mailbox for good: a 550, 551, or 553 whose enhanced code is 5.1, about
/// the address (RFC 3463). The same codes also refuse a sender or a quota, which a fix mends, so a reply without that
/// enhanced code is not final.
fn mailbox_refused(code: Option<&str>, reply: &str) -> bool {
    matches!(code, Some("550" | "551" | "553")) && reply.contains(" 5.1.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_refused_mailbox_is_final() {
        for (code, reply, refused) in [
            (
                Some("550"),
                "permanent error (550): 5.1.1 No such user",
                true,
            ),
            (Some("551"), "permanent error (551): 5.1.6 Moved", true),
            (
                Some("553"),
                "permanent error (553): 5.1.3 Bad address syntax",
                true,
            ),
            (
                Some("553"),
                "permanent error (553): 5.7.1 Sender address rejected",
                false,
            ),
            (
                Some("550"),
                "permanent error (550): 5.4.5 Daily sending limit exceeded",
                false,
            ),
            (
                Some("550"),
                "permanent error (550): Mailbox unavailable",
                false,
            ),
            (Some("554"), "permanent error (554): 5.1.1 Rejected", false),
            (None, "network error: 5.1.1", false),
        ] {
            assert_eq!(mailbox_refused(code, reply), refused, "{reply}");
        }
    }

    /// `mailbox` skips `lettre`'s own checks, which is safe only while every plain address passes them.
    #[test]
    fn lettre_accepts_every_plain_address() {
        for text in [
            "ada@example.com",
            "a!#$%&'*/=?^_`{|}~-z@mail.example-1.org",
            "ada@example.123",
            "x@a.bc",
            "ada@xn--bcher-kva.example",
            "ada@exa--mple.com",
            "-ada@example.com",
            &format!("{}@example.com", "a".repeat(64)),
            &format!("a@{}.com", "b".repeat(63)),
        ] {
            let address = Address::parse(text).unwrap();
            assert_eq!(
                text.to_ascii_lowercase()
                    .parse::<lettre::Address>()
                    .ok()
                    .map(|a| a.to_string()),
                Some(mailbox(&address).email.to_string().to_ascii_lowercase()),
                "{text}"
            );
        }
    }
}
