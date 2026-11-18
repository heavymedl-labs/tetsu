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
use tetsu_kernel::{Address, MailError, Message};

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

type Sending<'a> = Pin<Box<dyn Future<Output = Result<(), MailError>> + Send + 'a>>;

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
        Ok(Self::with_tls(settings, tls.ok_or(refused_host())?))
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
        let refused = format!("{}, or the PEM holds no certificate", refused_host());
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
    /// `Refused` if the server refuses the message for good, and `Unavailable` for anything trying again may fix,
    /// such as an outage, a refused login, or a send past the limit.
    pub(crate) async fn deliver(&self, message: &Message) -> Result<(), MailError> {
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
        let refusal: Sending<'_> = Box::pin(ready(Err(MailError::Refused)));
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
            .unwrap_or(Err(MailError::Unavailable))
    }
}

/// Names what is wrong without repeating the host, which a mistaken setting could fill with a login.
fn refused_host() -> String {
    "the SMTP host is not a name TLS can check".to_string()
}

/// The mailbox for a plain address, with no display name. A plain address is stricter than any `lettre` checks,
/// so it is built without checking again.
fn mailbox(address: &Address) -> Mailbox {
    Mailbox::new(
        None,
        lettre::Address::new_dangerous(address.local(), address.domain()),
    )
}

/// Only a refused mailbox (550, 551, or 553) is final. Every other failure, such as a refused login or sender, a
/// quota, a TLS failure, or an outage, is Tetsu's setup or the server's state, which a fix mends, so it is tried
/// again. Each is logged by its reply code and whether it timed out, failed TLS, or was permanent, which hold no
/// address and no secret.
fn sort(error: &SmtpError) -> MailError {
    let code = error.status().map(|code| code.to_string());
    let (timeout, tls, permanent) = (error.is_timeout(), error.is_tls(), error.is_permanent());
    tracing::warn!(
        code,
        timeout,
        tls,
        permanent,
        "mail: the SMTP server did not take a message"
    );
    match code.as_deref() {
        Some("550" | "551" | "553") => MailError::Refused,
        _ => MailError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
