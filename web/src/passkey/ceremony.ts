// One passkey ceremony with Tetsu's server: begin, ask the device, finish. Logging in, joining, and adding a
// passkey differ only in their endpoints and in which call they make on the device.

import { call } from "../shared/http";
import { assertion, type CreationJSON, creation, type RequestJSON, registration, request } from "./convert";

export type Action = "login" | "join" | "add";

const ENDPOINTS: Record<Action, string> = {
  login: "/ops/login",
  join: "/ops/join",
  add: "/ops/passkeys",
};

/** Runs one ceremony on `device`, sending `sent` with both halves: the invite link, or the operator's form
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
  const begun = await call(`${endpoint}/begin`, signal, sent);
  const signing = action === "login";
  const credential = signing
    ? await device.get(request(begun.options as RequestJSON, signal))
    : await device.create(creation(begun.options as CreationJSON, signal));
  // No passkey from the device is the browser's refusal, not the server's.
  if (credential === null) {
    throw new DOMException("the device gave no passkey", "NotAllowedError");
  }
  const passkey = credential as PublicKeyCredential;
  const response = signing ? assertion(passkey) : registration(passkey);
  // A prompt can answer just as a later press ends this ceremony; an ended one sends nothing more.
  signal.throwIfAborted();
  finishing();
  const finished = await call(`${endpoint}/finish`, signal, { ...sent, ceremony: begun.ceremony, response });
  if (typeof finished.next !== "string") {
    // The server answered, so it kept what it was sent; this is no refusal, only an answer the page cannot follow.
    throw new Error("the server said nowhere to go next");
  }
  return finished.next;
}
