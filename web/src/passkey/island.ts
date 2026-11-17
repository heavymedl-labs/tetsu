// The passkey islands (ADR-0031): light-DOM Web Components that add behavior to markup the server renders.
//
// - <tetsu-passkey action="sign-in|enroll|add" form-token="…"> wraps a button and a status line, and runs one
//   passkey ceremony when the button is pressed. Its state is an attribute, data-state, of ready, waiting, or failed,
//   for styles to read (ADR-0032).
// - <tetsu-passkeys user="…" credentials="…"> wraps the list of an operator's passkeys, and tells their devices every
//   passkey Tetsu holds, so a device can hide one Tetsu removed.

import { type Action, ceremony, Refused } from "./ceremony";

/** The parts of the browser the islands use, passed to `define` so tests can stand in for them. */
export interface Browser {
  location: Pick<Location, "hash" | "pathname" | "search" | "hostname" | "assign">;
  history: Pick<History, "replaceState">;
  credentials: CredentialsContainer | undefined;
  passkeys: typeof PublicKeyCredential | undefined;
}

const ACTIONS: readonly Action[] = ["sign-in", "enroll", "add"];

/** What the page says when a ceremony did not finish. A prompt ended by a newer one says nothing. */
export function said(error: unknown): string {
  if (error instanceof Refused) {
    return `${error.message.charAt(0).toUpperCase()}${error.message.slice(1)}.`;
  }
  const failed = "Something went wrong. Try again in a minute.";
  if (!(error instanceof DOMException)) {
    return failed;
  }
  if (error.name === "AbortError") {
    return "";
  }
  if (error.name === "NotAllowedError") {
    return "The passkey prompt closed without a passkey. Try again when you are ready.";
  }
  return failed;
}

/** What both halves of a ceremony send: an enrollment's link, which leaves the address bar at once, or the
 * operator's form token. */
function sentFor(action: Action, host: HTMLElement, browser: Browser): Record<string, string> {
  if (action === "enroll") {
    const link = browser.location.hash.slice(1);
    browser.history.replaceState(null, "", `${browser.location.pathname}${browser.location.search}`);
    return { link };
  }
  return action === "add" ? { form: host.getAttribute("form-token") ?? "" } : {};
}

/** Readies a `<tetsu-passkey>`, once: its button runs its action, if it names one and the browser can use
 * passkeys. */
export function ready(host: HTMLElement, browser: Browser): void {
  // Connected again, as when moved, an island already readied keeps its one listener.
  if (host.dataset.state !== undefined) {
    return;
  }
  const button = host.querySelector("button");
  const status = host.querySelector("[role=status]");
  const action = ACTIONS.find((known) => host.getAttribute("action") === known);
  if (button === null || status === null || action === undefined) {
    return;
  }
  const device = browser.credentials;
  if (browser.passkeys === undefined || device === undefined) {
    status.textContent = "This browser cannot use passkeys.";
    host.dataset.state = "failed";
    return;
  }
  const sent = sentFor(action, host, browser);
  button.disabled = false;
  host.dataset.state = "ready";
  button.addEventListener("click", async () => {
    status.textContent = "Waiting for your device…";
    host.dataset.state = "waiting";
    try {
      browser.location.assign(await ceremony(action, sent, device));
    } catch (error) {
      status.textContent = said(error);
      host.dataset.state = "failed";
    }
  });
}

/** Tells the operator's devices every passkey a `<tetsu-passkeys>` lists. A browser that cannot, or a device that
 * will not, changes nothing. */
export function signal(host: HTMLElement, browser: Browser): void {
  const passkeys = browser.passkeys;
  if (typeof passkeys?.signalAllAcceptedCredentials !== "function") {
    return;
  }
  passkeys
    .signalAllAcceptedCredentials({
      rpId: browser.location.hostname,
      userId: host.getAttribute("user") ?? "",
      allAcceptedCredentialIds: (host.getAttribute("credentials") ?? "").split(" ").filter(Boolean),
    })
    .catch(() => undefined);
}

/** Registers both islands, each readied as the parser connects it to the page. */
export function define(registry: CustomElementRegistry, browser: Browser): void {
  registry.define(
    "tetsu-passkey",
    class extends HTMLElement {
      connectedCallback(): void {
        ready(this, browser);
      }
    },
  );
  registry.define(
    "tetsu-passkeys",
    class extends HTMLElement {
      connectedCallback(): void {
        signal(this, browser);
      }
    },
  );
}
