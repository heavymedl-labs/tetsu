// The operator pages' script: it runs the one passkey action a page offers, and tells the person's devices which
// of their passkeys Tetsu still holds.

import { type Action, ceremony, Refused } from "./ceremony";

/** The parts of the page the script uses, passed in so tests can stand in for the browser's own. */
export interface Page {
  document: Document;
  location: Pick<Location, "hash" | "pathname" | "search" | "hostname" | "assign">;
  history: Pick<History, "replaceState">;
  credentials: CredentialsContainer | undefined;
  passkeys: typeof PublicKeyCredential | undefined;
}

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

/** What both halves of a page's ceremony send: an enrollment page's link, which leaves the address bar at once,
 * or the operator's form token. */
function sentFor(action: Action, button: HTMLButtonElement, page: Page): Record<string, string> {
  if (action === "enroll") {
    const link = page.location.hash.slice(1);
    page.history.replaceState(null, "", `${page.location.pathname}${page.location.search}`);
    return { link };
  }
  return action === "add" ? { form: button.dataset.form ?? "" } : {};
}

/** Tells the operator's devices every passkey Tetsu holds for them, so a device can hide one Tetsu removed. A
 * browser that cannot, or a device that will not, changes nothing. */
function signal(page: Page): void {
  const account = page.document.querySelector<HTMLElement>("[data-account]");
  const passkeys = page.passkeys;
  if (account === null || typeof passkeys?.signalAllAcceptedCredentials !== "function") {
    return;
  }
  passkeys
    .signalAllAcceptedCredentials({
      rpId: page.location.hostname,
      userId: account.dataset.user ?? "",
      allAcceptedCredentialIds: (account.dataset.credentials ?? "").split(" ").filter(Boolean),
    })
    .catch(() => undefined);
}

/** Readies the page's passkey button, if it has one and the browser can use passkeys. */
export function start(page: Page): void {
  signal(page);
  const button = page.document.querySelector<HTMLButtonElement>("button[data-passkey]");
  const status = page.document.querySelector<HTMLElement>("[data-passkey-status]");
  if (button === null || status === null) {
    return;
  }
  const device = page.credentials;
  if (page.passkeys === undefined || device === undefined) {
    status.textContent = "This browser cannot use passkeys.";
    return;
  }
  const action = button.dataset.passkey as Action;
  const sent = sentFor(action, button, page);
  button.disabled = false;
  button.addEventListener("click", async () => {
    status.textContent = "Waiting for your device…";
    try {
      page.location.assign(await ceremony(action, sent, device));
    } catch (error) {
      status.textContent = said(error);
    }
  });
}

start({
  document,
  location,
  history,
  credentials: navigator.credentials,
  passkeys: globalThis.PublicKeyCredential,
});
