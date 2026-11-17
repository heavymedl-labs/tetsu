// The passkey islands (ADR-0031): light-DOM Web Components that add behavior to markup the server renders.
//
// - <tetsu-passkey action="login|join|add" form-token="…" next="…"> wraps a button and a status line, and runs one
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

const ACTIONS: readonly Action[] = ["login", "join", "add"];

/** What the page says once an action is done, in the moment before the browser goes where the server says. An
 * join is not done until the login it chains to is (ADR-0067). */
const DONE: Record<Exclude<Action, "join">, string> = {
  login: "Logged in.",
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
  // The island's own ended ceremonies never reach here, so this is the browser ending the prompt by itself.
  if (error.name === "AbortError") {
    return "The passkey prompt closed before it finished. Try again.";
  }
  if (error.name === "InvalidStateError") {
    return "Finish or close the passkey prompt that is already open, then try again.";
  }
  if (error.name === "NotAllowedError") {
    return "The passkey prompt closed without a passkey. Try again when you are ready.";
  }
  return failed;
}

/** What both halves of a ceremony send: a join's link, which leaves the address bar at once, the operator's form
 * token, or the page a login returns to, which the server checks again. */
function sentFor(action: Action, host: HTMLElement, browser: Browser): Record<string, string> {
  if (action === "join") {
    const link = browser.location.hash.slice(1);
    browser.history.replaceState(null, "", `${browser.location.pathname}${browser.location.search}`);
    return { link };
  }
  if (action === "add") {
    return { form: host.getAttribute("form-token") ?? "" };
  }
  const next = host.getAttribute("next");
  return next === null ? {} : { next };
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
 * passkeys. A join's link then leaves the address bar at once. */
export function ready(host: HTMLElement, browser: Browser): void {
  // Connected again, as when moved, an island already readied keeps its one listener.
  if (host.dataset.state !== undefined) {
    return;
  }
  const parts = partsOf(host);
  if (parts === undefined) {
    return;
  }
  const device = browser.credentials;
  if (browser.passkeys === undefined || device === undefined) {
    // A join's link stays in the address bar, so the person can open it in a browser that can.
    parts.status.textContent =
      parts.action === "join"
        ? "This browser cannot use passkeys. Open this link in one that can."
        : "This browser cannot use passkeys.";
    host.dataset.state = "failed";
    return;
  }
  const sent = sentFor(parts.action, host, browser);
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

/** How long a device may take: after the first, the page says it is still waiting; after the second, the island
 * closes the prompt itself, since a device that finds no passkey says nothing, and browsers treat the server's own
 * limit of the same length only as a hint. Neither runs once a ceremony's last request is on its way. */
const HINT_AFTER = 15_000;
const GIVE_UP_AFTER = 60_000;

/** Why the island ended a ceremony itself, as against a later press ending it. */
const TIMED_OUT = "timed out";

/** The island's presses: each runs its action and says how it goes. A press ends any ceremony still under way,
 * and only the latest press speaks: an ended one never writes to the page or moves it. */
class Presses {
  private action: Action;
  private sent: Record<string, string>;
  private pending: AbortController | undefined;
  /** The presses, by their signals, whose last request went out, after which the server may have kept what it
   * was sent. */
  private readonly lastSent = new WeakSet<AbortSignal>();
  /** Stops the clock on the ceremony under way. */
  private stopClock: () => void = () => undefined;

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
    this.startClock(controller);
    try {
      const next = await ceremony(this.action, this.sent, this.island.device, controller.signal, () => {
        this.lastSent.add(controller.signal);
        this.finishing();
      });
      if (this.action === "join") {
        await this.chain(controller);
      } else {
        this.arrive(this.action, next);
      }
    } catch (error) {
      const timedOut = controller.signal.reason === TIMED_OUT;
      // Ended by a later press, which now owns the clock and the page.
      if (controller.signal.aborted && !timedOut) {
        return;
      }
      this.stopClock();
      // A join whose last request went out but whose answer was lost may have kept the passkey and spent
      // the link, so its link is no use again; logging in is. A refusal means the server kept nothing.
      if (this.action === "join" && this.lastSent.has(controller.signal) && !(error instanceof Refused)) {
        this.toLogin();
        this.say("Your passkey may have been created. Continue with passkey to log in.", "ready");
        return;
      }
      this.say(
        timedOut
          ? "Your device did not answer within a minute, so the prompt was closed. Try again, or use the device you set up."
          : said(error),
        "failed",
      );
    }
  }

  /** Starts the clock on a ceremony, stopping any before it: a hint once it is slow, and its end once it is too
   * slow. */
  private startClock(controller: AbortController): void {
    this.stopClock();
    const hint = setTimeout(
      () =>
        this.say(
          "Still waiting. If your device can't find or make a passkey for Tetsu, cancel and use the one you set up, or ask an operator for a new invite.",
          "waiting",
        ),
      HINT_AFTER,
    );
    const end = setTimeout(() => controller.abort(TIMED_OUT), GIVE_UP_AFTER);
    this.stopClock = () => {
      clearTimeout(hint);
      clearTimeout(end);
    };
  }

  /** Turns the island to logging in, for good: a join's link is spent once its passkey is kept. */
  private toLogin(): void {
    this.action = "login";
    this.sent = {};
    this.island.parts.label.textContent = "Continue with passkey";
  }

  // An operator's session comes from a passkey login, never from the link that joined them, so a new passkey
  // logs in at once, on this page (ADR-0067). A browser may refuse a prompt the person did not start by pressing
  // something; the button then logs in instead. A refusal from the server is said as any other.
  private async chain(controller: AbortController): Promise<void> {
    const signal = controller.signal;
    this.toLogin();
    this.say("Passkey created. Confirm once more to log in.", "waiting");
    this.startClock(controller);
    try {
      this.arrive("login", await ceremony("login", this.sent, this.island.device, signal, this.finishing));
    } catch (error) {
      // A chained login that timed out leaves a passkey made, so the button logs in, rather than failing.
      if (error instanceof Refused || (signal.aborted && signal.reason !== TIMED_OUT)) {
        throw error;
      }
      this.stopClock();
      this.say("Passkey created. Continue with passkey to log in.", "ready");
    }
  }

  // Once a ceremony's last request is on its way, the server may keep what it sent, as a passkey and a spent link,
  // so no press may end it: the button stays off until the next thing said.
  private readonly finishing = (): void => {
    this.stopClock();
    this.island.parts.button.disabled = true;
  };

  private arrive(done: Exclude<Action, "join">, next: string): void {
    this.stopClock();
    this.say(DONE[done], "done");
    this.island.browser.location.assign(next);
  }

  /** Says how it goes, and turns the button back on for anything but done, while the page moves on. */
  private say(words: string, state: string): void {
    this.island.parts.status.textContent = words;
    this.island.host.dataset.state = state;
    this.island.parts.button.disabled = state === "done";
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

/** Registers both islands. The module is deferred, so each is readied as soon as it runs, for the markup already
 * parsed. */
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
