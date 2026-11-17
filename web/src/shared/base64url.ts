// Base64url without padding (RFC 4648 section 5), the form WebAuthn's JSON uses for credential ids, challenges,
// and responses. Browsers gained Uint8Array.fromBase64 only in Safari 18.2, above the browser floor (ADR-0053).

const alphabet = /^[A-Za-z0-9_-]*$/;

export function encode(data: ArrayBuffer | Uint8Array): string {
  // A view copies only its own bytes, so a subarray never brings the rest of its buffer.
  const binary = Array.from(new Uint8Array(data), (byte) => String.fromCharCode(byte)).join("");
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

export function decode(text: string): Uint8Array<ArrayBuffer> {
  // A length of 1 more than a multiple of 4 holds 6 bits, less than a byte, so no encoder writes it.
  if (!alphabet.test(text) || text.length % 4 === 1) {
    throw new TypeError("not base64url");
  }
  const binary = atob(text.replaceAll("-", "+").replaceAll("_", "/"));
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  // atob ignores the unused bits of the last character, so "Zh" would read as "Zg". Each value has one spelling.
  if (encode(bytes) !== text) {
    throw new TypeError("not base64url");
  }
  return bytes;
}
