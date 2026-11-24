// One passkey ceremony with Tetsu's server: begin, ask the device, finish. Logging in, joining, and adding a
// passkey differ only in their endpoints and in which call they make on the device.

import { call } from "../shared/http";
import { assertion, type CreationJSON, creation, type RequestJSON, registration, request } from "./convert";

export type Action = "login" | "join" | "add";

/** What a ceremony is asked to do: its action, in the realm at `base`, sending `sent` with both halves. */
export interface Asked {
  action: Action;
  base: string;
  sent: Record<string, string>;
}

/** Where each action's requests go, below its realm's base path. */
const ENDPOINTS: Record<Action, string> = {
  login: "/login",
  join: "/join",
  add: "/passkeys",
};

/** Runs one ceremony on `device`, as `asked`, sending its `sent` with both halves: the invite link, or the person's
 * form token. `signal` ends it, its requests and the device's prompt alike, as when the person presses again, until
 * `finishing` is called, just before the last request: from then the server may have kept what it sent, so the
 * caller lets nothing end it. Returns where the page goes once it is done. */
export async function ceremony(
  { action, base, sent }: Asked,
  device: CredentialsContainer,
  signal: AbortSignal,
  finishing: () => void,
): Promise<string> {
  const endpoint = `${base}${ENDPOINTS[action]}`;
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
