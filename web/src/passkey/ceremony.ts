// One passkey ceremony with Tetsu's server: begin, ask the device, finish. Signing in, enrolling, and adding a
// passkey differ only in their endpoints and in which call they make on the device.

import { assertion, type CreationJSON, creation, type RequestJSON, registration, request } from "./convert";

export type Action = "sign-in" | "enroll" | "add";

const ENDPOINTS: Record<Action, string> = {
  "sign-in": "/ops/sign-in",
  enroll: "/ops/enroll",
  add: "/ops/passkeys",
};

/** A refusal the server gave, in words a page may show. */
export class Refused extends Error {}

async function call(path: string, body: object, signal: AbortSignal): Promise<Record<string, unknown>> {
  const response = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal,
  });
  // A proxy's or rate limiter's answer may not be JSON; a refusal is still the server's.
  const json = (await response.json().catch(() => ({}))) as Record<string, unknown>;
  if (!response.ok) {
    throw new Refused(typeof json.error === "string" ? json.error : "this request was refused");
  }
  return json;
}

/** Runs one ceremony on `device`, sending `sent` with both halves: the enrollment link, or the operator's form
 * token. `signal` ends it, its requests and the device's prompt alike, as when the person presses again, until
 * `finishing` is called, just before the last request: from then the server may have kept what it sent, so the
 * caller lets nothing end it. Returns where the page goes once it is done. */
export async function ceremony(
  action: Action,
  sent: Record<string, string>,
  device: CredentialsContainer,
  signal: AbortSignal,
  finishing: () => void,
): Promise<string> {
  const endpoint = ENDPOINTS[action];
  const begun = await call(`${endpoint}/begin`, sent, signal);
  const signing = action === "sign-in";
  const credential = signing
    ? await device.get(request(begun.options as RequestJSON, signal))
    : await device.create(creation(begun.options as CreationJSON, signal));
  // No passkey from the device is the browser's refusal, not the server's.
  if (credential === null) {
    throw new DOMException("the device gave no passkey", "NotAllowedError");
  }
  const passkey = credential as PublicKeyCredential;
  const response = signing ? assertion(passkey) : registration(passkey);
  finishing();
  const finished = await call(`${endpoint}/finish`, { ...sent, ceremony: begun.ceremony, response }, signal);
  if (typeof finished.next !== "string") {
    throw new Refused("the server said nowhere to go next");
  }
  return finished.next;
}
