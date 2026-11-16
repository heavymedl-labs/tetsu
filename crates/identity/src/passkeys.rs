//! Passkey ceremonies through `webauthn-rs` (ADR-0047). Its types stay in this file: the rest of Identity sees only
//! JSON text, for what goes to and from the browser and for the state kept in Postgres between a ceremony's halves.

use serde_json::{Value, json};
use webauthn_rs::prelude::{
    PasskeyRegistration, RegisterPublicKeyCredential, Url, Uuid, Webauthn, WebauthnBuilder,
};

use crate::domain::{Error, NewPasskey};

/// The relying party a realm's passkeys belong to: its origin, exactly, and that origin's host as the relying party
/// ID. A response from any other origin, port, or subdomain is refused.
pub struct RelyingParty {
    webauthn: Webauthn,
}

impl RelyingParty {
    /// The relying party for an origin such as `https://tetsu-demo.onrender.com`.
    ///
    /// # Errors
    /// Fails with [`Error::Invalid`] for text that is not an origin with a host.
    pub fn new(origin: &str) -> Result<Self, Error> {
        let not_an_origin = || Error::Invalid(format!("{origin:?} is not an origin"));
        let url = Url::parse(origin).map_err(|_| not_an_origin())?;
        let host = url.host_str().ok_or_else(not_an_origin)?;
        let webauthn = WebauthnBuilder::new(host, &url)
            .and_then(|builder| builder.rp_name("Tetsu").build())
            .map_err(|_| not_an_origin())?;
        Ok(Self { webauthn })
    }

    /// Starts registering a passkey for a user handle and name: the options for the browser, and the state to keep
    /// until it answers. The passkey must verify the user, and must be discoverable, so signing in needs no name.
    /// Every realm shares one passkey domain until Tetsu has domains (ADR-0061), so the name a device's account
    /// picker shows says the passkey is an operator's.
    pub(crate) fn begin_registration(
        &self,
        user_handle: [u8; 16],
        name: &str,
    ) -> Result<(String, String), Error> {
        let (options, state) = self
            .webauthn
            .start_passkey_registration(
                Uuid::from_bytes(user_handle),
                &format!("{name} (Tetsu operator)"),
                name,
                None,
            )
            .map_err(|error| Error::Invalid(format!("no ceremony could start: {error}")))?;
        // Serialising these types cannot fail, so json! takes the place of a Result no test could reach.
        let mut options = json!(options);
        options["publicKey"]["authenticatorSelection"]["residentKey"] = json!("required");
        options["publicKey"]["authenticatorSelection"]["requireResidentKey"] = Value::Bool(true);
        Ok((options.to_string(), json!(state).to_string()))
    }

    /// Verifies a browser's registration against the state its ceremony began with, and returns the passkey. A
    /// registration whose client says the passkey is not discoverable is refused, since signing in could never find
    /// it; one that says nothing is taken, as some browsers say nothing. So is one too large to store: a credential
    /// ID outside the 16 to 1023 bytes `WebAuthn` allows, or a passkey larger than the store keeps.
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
        let response: Value = serde_json::from_str(response).map_err(|_| refused())?;
        if response["clientExtensionResults"]["credProps"]["rk"] == json!(false) {
            return Err(refused());
        }
        let response: RegisterPublicKeyCredential =
            serde_json::from_value(response).map_err(|_| refused())?;
        let passkey = self
            .webauthn
            .finish_passkey_registration(&response, &state)
            .map_err(|_| refused())?;
        Some(NewPasskey {
            credential_id: AsRef::<[u8]>::as_ref(passkey.cred_id()).to_vec(),
            passkey: json!(passkey).to_string(),
        })
        .filter(fits)
        .ok_or_else(refused)
    }
}

/// Whether a passkey fits what the store keeps: a credential ID of 16 to 1023 bytes, as `WebAuthn` sets, and at most
/// 8 KiB of passkey, as the table checks.
fn fits(passkey: &NewPasskey) -> bool {
    (16..=1023).contains(&passkey.credential_id.len()) && passkey.passkey.len() <= 8192
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bounds are those of `WebAuthn` and the table, inclusive.
    #[test]
    fn a_passkey_fits_only_within_the_stores_bounds() {
        let sized = |id: usize, json: usize| {
            fits(&NewPasskey {
                credential_id: vec![0; id],
                passkey: "x".repeat(json),
            })
        };
        assert_eq!(
            [
                sized(16, 8192),
                sized(1023, 1),
                sized(15, 1),
                sized(1024, 1),
                sized(16, 8193)
            ],
            [true, true, false, false, false]
        );
    }
}
