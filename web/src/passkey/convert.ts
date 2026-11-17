// Between the JSON Tetsu's server speaks, as webauthn-rs writes and reads it, and the browser's WebAuthn calls.
// PublicKeyCredential's own JSON methods came in Safari 18.4, above the browser floor (ADR-0053), so this does
// their work with base64url.

import { decode, encode } from "../shared/base64url";

/** Options to create a passkey, as the server sends them. */
export interface CreationJSON {
  publicKey: PublicKeyCredentialCreationOptionsJSON;
}

/** Options to log in, as the server sends them. */
export interface RequestJSON {
  publicKey: PublicKeyCredentialRequestOptionsJSON;
}

function descriptor(json: PublicKeyCredentialDescriptorJSON): PublicKeyCredentialDescriptor {
  return {
    type: "public-key",
    id: decode(json.id),
    transports: (json.transports ?? []) as AuthenticatorTransport[],
  };
}

/** The options `navigator.credentials.create` takes, ended by `signal`. */
export function creation(json: CreationJSON, signal: AbortSignal): CredentialCreationOptions {
  const options = json.publicKey;
  return {
    signal,
    publicKey: {
      ...options,
      challenge: decode(options.challenge),
      user: { ...options.user, id: decode(options.user.id) },
      excludeCredentials: (options.excludeCredentials ?? []).map(descriptor),
    } as unknown as PublicKeyCredentialCreationOptions,
  };
}

/** The options `navigator.credentials.get` takes, ended by `signal`. A button opens the prompt, so it asks for
 * no autofill. */
export function request(json: RequestJSON, signal: AbortSignal): CredentialRequestOptions {
  const options = json.publicKey;
  return {
    signal,
    publicKey: {
      ...options,
      challenge: decode(options.challenge),
      allowCredentials: (options.allowCredentials ?? []).map(descriptor),
    } as unknown as PublicKeyCredentialRequestOptions,
  };
}

/** A new passkey, as the server reads it. A browser that cannot list its transports lists none. */
export function registration(credential: PublicKeyCredential): object {
  const response = credential.response as AuthenticatorAttestationResponse;
  return {
    id: credential.id,
    rawId: encode(credential.rawId),
    type: credential.type,
    response: {
      clientDataJSON: encode(response.clientDataJSON),
      attestationObject: encode(response.attestationObject),
      transports: typeof response.getTransports === "function" ? response.getTransports() : [],
    },
    clientExtensionResults: credential.getClientExtensionResults(),
  };
}

/** A signed login, as the server reads it. */
export function assertion(credential: PublicKeyCredential): object {
  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    id: credential.id,
    rawId: encode(credential.rawId),
    type: credential.type,
    response: {
      clientDataJSON: encode(response.clientDataJSON),
      authenticatorData: encode(response.authenticatorData),
      signature: encode(response.signature),
      userHandle: response.userHandle === null ? null : encode(response.userHandle),
    },
    clientExtensionResults: credential.getClientExtensionResults(),
  };
}
