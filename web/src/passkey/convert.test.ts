import { describe, expect, it } from "vitest";
import { decode } from "../shared/base64url";
import { assertion, type CreationJSON, creation, type RequestJSON, registration, request } from "./convert";
import fixture from "./fixtures/options.json";

// The fixture is real webauthn-rs options, which a Rust test keeps in shape (crates/identity/src/http_tests.rs).
const created = fixture.creation as unknown as CreationJSON;
const requested = fixture.request as unknown as RequestJSON;
const signal = new AbortController().signal;

const bytes = (...values: number[]) => new Uint8Array(values);

function credential(response: object): PublicKeyCredential {
  return {
    id: "AQI",
    rawId: bytes(1, 2).buffer,
    type: "public-key",
    response,
    getClientExtensionResults: () => ({ credProps: { rk: true } }),
  } as unknown as PublicKeyCredential;
}

describe("creation", () => {
  const options = creation(created, signal);
  const publicKey = options.publicKey as PublicKeyCredentialCreationOptions;

  it("reads the challenge, the user handle, and each passkey held as bytes", () => {
    expect(publicKey.challenge).toEqual(decode(created.publicKey.challenge));
    expect(publicKey.user.id).toEqual(decode(created.publicKey.user.id));
    const held = created.publicKey.excludeCredentials?.[0]?.id ?? "";
    expect(publicKey.excludeCredentials).toEqual([
      { type: "public-key", id: decode(held), transports: ["hybrid", "internal"] },
    ]);
  });

  it("keeps the rest as the server sent it, and the signal that ends it", () => {
    expect(options.signal).toBe(signal);
    expect(publicKey.user.name).toBe(created.publicKey.user.name);
    expect(publicKey.authenticatorSelection).toEqual(created.publicKey.authenticatorSelection);
    expect(publicKey.rp).toEqual({ name: "Tetsu", id: "ops.example.com" });
  });

  it("excludes nothing, with no transports, when the server lists none", () => {
    const { excludeCredentials: _, ...rest } = created.publicKey;
    const bare = creation({ publicKey: rest }, signal).publicKey as PublicKeyCredentialCreationOptions;
    expect(bare.excludeCredentials).toEqual([]);
    const held = { ...rest, excludeCredentials: [{ type: "public-key", id: "AQI" }] };
    const listed = creation({ publicKey: held }, signal).publicKey as PublicKeyCredentialCreationOptions;
    expect(listed.excludeCredentials).toEqual([{ type: "public-key", id: bytes(1, 2), transports: [] }]);
  });
});

describe("request", () => {
  it("reads the challenge as bytes, names no passkey, and keeps the rest and the signal", () => {
    const options = request(requested, signal);
    const publicKey = options.publicKey as PublicKeyCredentialRequestOptions;
    expect(options.signal).toBe(signal);
    expect(publicKey.challenge).toEqual(decode(requested.publicKey.challenge));
    expect(publicKey.allowCredentials).toEqual([]);
    expect(publicKey.userVerification).toBe("required");
    expect(publicKey.rpId).toBe("ops.example.com");
  });

  it("names nothing when the server leaves the list out", () => {
    const { allowCredentials: _, ...rest } = requested.publicKey;
    expect(
      (request({ publicKey: rest }, signal).publicKey as PublicKeyCredentialRequestOptions).allowCredentials,
    ).toEqual([]);
  });
});

describe("registration", () => {
  const attested = {
    clientDataJSON: bytes(3).buffer,
    attestationObject: bytes(4).buffer,
    getTransports: () => ["internal", "hybrid"],
  };

  it("writes each part as base64url, with the transports the browser lists", () => {
    expect(registration(credential(attested))).toEqual({
      id: "AQI",
      rawId: "AQI",
      type: "public-key",
      response: { clientDataJSON: "Aw", attestationObject: "BA", transports: ["internal", "hybrid"] },
      clientExtensionResults: { credProps: { rk: true } },
    });
  });

  it("lists no transports when the browser cannot", () => {
    const { getTransports: _, ...older } = attested;
    expect(registration(credential(older))).toMatchObject({ response: { transports: [] } });
  });
});

describe("assertion", () => {
  const signed = {
    clientDataJSON: bytes(3).buffer,
    authenticatorData: bytes(5).buffer,
    signature: bytes(6).buffer,
    userHandle: bytes(7).buffer,
  };

  it("writes each part as base64url", () => {
    expect(assertion(credential(signed))).toEqual({
      id: "AQI",
      rawId: "AQI",
      type: "public-key",
      response: { clientDataJSON: "Aw", authenticatorData: "BQ", signature: "Bg", userHandle: "Bw" },
      clientExtensionResults: { credProps: { rk: true } },
    });
  });

  it("sends no user handle when the authenticator gave none", () => {
    expect(assertion(credential({ ...signed, userHandle: null }))).toMatchObject({
      response: { userHandle: null },
    });
  });
});
