// The passkey islands (ADR-0031): light-DOM Web Components that add behavior to markup the server renders.
//
// - <tetsu-passkey action="sign-in|enroll|add" form-token="…"> wraps a button and a status line, and runs one
//   passkey ceremony when the button is pressed. Its state is an attribute, data-state, of ready, waiting, done, or
//   failed, for styles to read (ADR-0032).
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

/** What the page says once an action is done, in the moment before the browser goes where the server says. An
 * enrollment is not done until the sign-in it chains to is (ADR-0067). */
const DONE: Record<Exclude<Action, "enroll">, string> = {
  "sign-in": "Signed in.",
  add: "Passkey added.",
};

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

/** The parts of a `<tetsu-passkey>` it works through, if it has them all and names an action it knows. */
interface Parts {
  button: HTMLButtonElement;
  status: Element;
  label: Element;
  action: Action;
}

function partsOf(host: HTMLElement): Parts | undefined {
  const button = host.querySelector("button");
  const status = host.querySelector("[role=status]");
  const label = host.querySelector("[data-label]");
  const action = ACTIONS.find((known) => host.getAttribute("action") === known);
  if (button === null || status === null || label === null || action === undefined) {
    return undefined;
  }
  return { button, status, label, action };
}

/** Readies a `<tetsu-passkey>`, once: its button runs its action, if it has its parts and the browser can use
 * passkeys. An enrollment's link leaves the address bar first, whatever follows. */
export function ready(host: HTMLElement, browser: Browser): void {
  // Connected again, as when moved, an island already readied keeps its one listener.
  if (host.dataset.state !== undefined) {
    return;
  }
  const parts = partsOf(host);
  if (parts === undefined) {
    return;
  }
  const sent = sentFor(parts.action, host, browser);
  const device = browser.credentials;
  if (browser.passkeys === undefined || device === undefined) {
    parts.status.textContent = "This browser cannot use passkeys.";
    host.dataset.state = "failed";
    return;
  }
  const presses = new Presses({ host, parts, browser, device }, sent);
  parts.button.addEventListener("click", () => presses.press());
  parts.button.disabled = false;
  host.dataset.state = "ready";
}

/** What a readied island works with. */
interface Island {
  host: HTMLElement;
  parts: Parts;
  browser: Browser;
  device: CredentialsContainer;
}

/** The island's presses: each runs its action and says how it goes. A press ends any ceremony still under way,
 * and only the latest press speaks: an ended one never writes to the page or moves it. */
class Presses {
  private action: Action;
  private sent: Record<string, string>;
  private pending: AbortController | undefined;

  constructor(
    private readonly island: Island,
    sent: Record<string, string>,
  ) {
    this.action = island.parts.action;
    this.sent = sent;
  }

  async press(): Promise<void> {
    this.pending?.abort();
    const controller = new AbortController();
    this.pending = controller;
    this.say("Waiting for your device…", "waiting");
    try {
      const next = await ceremony(this.action, this.sent, this.island.device, controller.signal);
      if (this.action === "enroll") {
        await this.chain(controller.signal);
      } else {
        this.arrive(this.action, next);
      }
    } catch (error) {
      if (!controller.signal.aborted) {
        this.say(said(error), "failed");
      }
    }
  }

  // An operator's session comes from a passkey sign-in, never from the link that enrolled them, so a new passkey
  // signs in at once, on this page (ADR-0067). A browser may refuse a prompt the person did not start by pressing
  // something; the button then signs in instead. A refusal from the server is said as any other.
  private async chain(signal: AbortSignal): Promise<void> {
    this.action = "sign-in";
    this.sent = {};
    this.island.parts.label.textContent = "Continue with passkey";
    this.say("Passkey created. Confirm once more to sign in.", "waiting");
    try {
      this.arrive("sign-in", await ceremony("sign-in", this.sent, this.island.device, signal));
    } catch (error) {
      if (error instanceof Refused || signal.aborted) {
        throw error;
      }
      this.say("Passkey created. Continue with passkey to sign in.", "ready");
    }
  }

  private arrive(done: Exclude<Action, "enroll">, next: string): void {
    this.say(DONE[done], "done");
    this.island.parts.button.disabled = true;
    this.island.browser.location.assign(next);
  }

  private say(words: string, state: string): void {
    this.island.parts.status.textContent = words;
    this.island.host.dataset.state = state;
  }
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
