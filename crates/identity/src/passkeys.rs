//! Passkey ceremonies through `webauthn-rs` (ADR-0047). Its types stay in this file, as far as Identity's own code
//! goes (tests borrow its `Url` for the authenticator that stands in for a person): the rest of Identity sees only
//! JSON text, for what goes to and from the browser and for the state kept in Postgres between a ceremony's halves.

use serde_json::{Value, json};
use webauthn_rs::prelude::{
    PasskeyRegistration, RegisterPublicKeyCredential, Url, Uuid, Webauthn, WebauthnBuilder,
};

use crate::browser::Origins;
use crate::domain::{Error, NewPasskey};

/// What an account picker shows before an operator's name. Authenticators may keep only the first 64 bytes of a
/// name (`WebAuthn` L3 §6.4.1), so the label comes first, where a cut never reaches it.
const LABEL: &str = "Tetsu operator · ";

/// The relying party a realm's passkeys belong to: its origin's scheme, host, and port, which `webauthn-rs` compares
/// (a path or query a client adds is not), and that origin's host as the relying party ID. A response from any
/// other scheme, port, or subdomain is refused.
pub struct RelyingParty {
    webauthn: Webauthn,
}

impl RelyingParty {
    /// The relying party for an origin such as `https://tetsu-demo.onrender.com`, written as browsers send it, with
    /// [`Origins`]'s rules: `https`, or `http` for this machine alone, a host of plain labels, and no path. The host
    /// must be a domain, since passkeys cannot belong to an address.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for text that is not such an origin.
    pub fn new(origin: &str) -> Result<Self, Error> {
        let not_an_origin = || Error::Invalid(format!("{origin:?} is not an origin"));
        // Text Origins accepts may still fail as a URL, such as an address out of range, so both checks must pass.
        let written_as_browsers_send = Origins::parse(&[origin]).is_ok();
        let url = Url::parse(origin)
            .ok()
            .filter(|_| written_as_browsers_send)
            .ok_or_else(not_an_origin)?;
        let webauthn = WebauthnBuilder::new(url.host_str().unwrap_or_default(), &url)
            .and_then(|builder| builder.rp_name("Tetsu").build())
            .map_err(|_| not_an_origin())?;
        Ok(Self { webauthn })
    }

    /// Starts registering a passkey for a user handle and name: the options for the browser, and the state to keep
    /// until it answers. The passkey must verify the user, and must be discoverable, so signing in needs no name.
    /// Every realm shares one passkey domain until Tetsu has domains (ADR-0061), so the name a device's account
    /// picker shows says the passkey is an operator's. It fails only for a stored operator with no name, a fault of
    /// storage, never the browser's.
    pub(crate) fn begin_registration(
        &self,
        user_handle: [u8; 16],
        name: &str,
    ) -> Result<(String, String), Error> {
        let (options, state) = self
            .webauthn
            .start_passkey_registration(
                Uuid::from_bytes(user_handle),
                &account_name(name),
                name,
                None,
            )
            .map_err(|error| Error::Storage(format!("no ceremony could start: {error}")))?;
        // Serialising these types cannot fail, so json! takes the place of a Result no test could reach.
        let mut options = json!(options);
        options["publicKey"]["authenticatorSelection"]["residentKey"] = json!("required");
        options["publicKey"]["authenticatorSelection"]["requireResidentKey"] = Value::Bool(true);
        Ok((options.to_string(), json!(state).to_string()))
    }

    /// Verifies a browser's registration against the state its ceremony began with, and returns the passkey. A
    /// registration whose client says the passkey is not discoverable is refused, since signing in could never find
    /// it; one that says nothing is taken, as some browsers say nothing. So is one listing a transport `WebAuthn`
    /// does not name, or one twice. Its size is checked where it is stored, measured as the table measures it.
    pub(crate) fn finish_registration(
        &self,
        response: &str,
        state: &str,
    ) -> Result<NewPasskey, Error> {
        // The stored state first, so a fault of the server's is never reported as the browser's.
        let state: PasskeyRegistration = serde_json::from_str(state).map_err(|error| {
            Error::Storage(format!(
                "a stored passkey ceremony could not be read: {error}"
            ))
        })?;
        let refused = || Error::Invalid("the passkey could not be verified".to_string());
        // Parsed first, so each check reads what webauthn-rs reads, whichever name the client gave a field.
        let response = serde_json::from_str::<RegisterPublicKeyCredential>(response)
            .ok()
            .filter(plausible)
            .ok_or_else(refused)?;
        let passkey = self
            .webauthn
            .finish_passkey_registration(&response, &state)
            .map_err(|_| refused())?;
        Ok(NewPasskey {
            credential_id: AsRef::<[u8]>::as_ref(passkey.cred_id()).to_vec(),
            passkey: json!(passkey).to_string(),
        })
    }
}

/// Whether a registration could be a usable passkey: not one its client says is not discoverable, and listing only
/// transports `WebAuthn` names, each once, so at most six.
fn plausible(response: &RegisterPublicKeyCredential) -> bool {
    let discoverable = response
        .extensions
        .cred_props
        .as_ref()
        .and_then(|props| props.rk)
        != Some(false);
    let transports = response.response.transports.as_deref().unwrap_or_default();
    let known = transports.iter().enumerate().all(|(at, transport)| {
        transport.to_string() != "unknown" && !transports[..at].contains(transport)
    });
    discoverable && known
}

/// The name an account picker shows for an operator's passkey: the label, then their name, cut to the 64 bytes an
/// authenticator may keep, at a character's end.
fn account_name(name: &str) -> String {
    let mut shown = String::from(LABEL);
    for character in name.chars() {
        if shown.len() + character.len_utf8() > 64 {
            break;
        }
        shown.push(character);
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label always survives, and a long name is cut at a character's end, within 64 bytes.
    #[test]
    fn an_account_name_keeps_its_label_within_64_bytes() {
        assert_eq!(account_name("Kurt Medley"), "Tetsu operator · Kurt Medley");
        // The label is 18 bytes, leaving 46: fifteen three-byte characters fill 45, and a sixteenth would not fit.
        let long = account_name(&"あ".repeat(100));
        assert_eq!((long.len(), long.starts_with(LABEL)), (63, true));
        assert_eq!(account_name(&"a".repeat(100)).len(), 64);
    }
}
