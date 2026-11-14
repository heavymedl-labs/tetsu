import { describe, expect, it } from "vitest";
import { decode, encode } from "./base64url";

// The vectors are ASCII, so each character is one byte. TextEncoder would build its array in jsdom's realm,
// which deep equality tells apart from this one's.
const bytes = (text: string) => Uint8Array.from(text, (char) => char.charCodeAt(0));

// RFC 4648 section 10's test vectors, in the URL-safe alphabet without padding, as WebAuthn writes them.
const vectors: [string, string][] = [
  ["", ""],
  ["f", "Zg"],
  ["fo", "Zm8"],
  ["foo", "Zm9v"],
  ["foob", "Zm9vYg"],
  ["fooba", "Zm9vYmE"],
  ["foobar", "Zm9vYmFy"],
];

describe("encode", () => {
  it.each(vectors)("writes %j as %j", (text, encoded) => {
    expect(encode(bytes(text))).toBe(encoded);
  });

  it("uses - and _ where base64 uses + and /", () => {
    expect(encode(new Uint8Array([0xfb, 0xff]))).toBe("-_8");
  });

  it("reads an ArrayBuffer, as WebAuthn returns one", () => {
    expect(encode(bytes("foobar").buffer)).toBe("Zm9vYmFy");
  });

  it("reads only a view's own bytes, not the rest of its buffer", () => {
    expect(encode(bytes("xfoobarx").subarray(1, 7))).toBe("Zm9vYmFy");
  });
});

describe("decode", () => {
  it.each(vectors)("reads %j from %j", (text, encoded) => {
    expect(decode(encoded)).toEqual(bytes(text));
  });

  it("reads - and _", () => {
    expect(decode("-_8")).toEqual(new Uint8Array([0xfb, 0xff]));
  });

  it("returns every byte value it was given", () => {
    const all = Uint8Array.from({ length: 256 }, (_, i) => i);
    expect(decode(encode(all))).toEqual(all);
  });

  // A credential id or challenge the server sent malformed must fail here, not reach the authenticator.
  it.each([
    ["padding", "Zg=="],
    ["base64's +", "+_8"],
    ["base64's /", "-/8"],
    ["a space", "Zm9 v"],
    ["a line break", "Zm9v\nYg"],
    ["a character outside the alphabet", "Zm9v!"],
    ["a first character outside the alphabet", "!m9v"],
    ["a last character outside the alphabet", "Zm9!"],
    ["a length no bytes can have", "Zm9vY"],
    // RFC 4648 section 3.5: the unused bits of the last character must be zero, so each value has one spelling.
    ["leftover bits that are not zero", "Zh"],
  ])("refuses %s", (_, encoded) => {
    // The class too, so a malformed value never reaches atob, whose own error would read differently.
    expect(() => decode(encoded)).toThrow(TypeError);
    expect(() => decode(encoded)).toThrow("not base64url");
  });
});
