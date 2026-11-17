// The session island (ADR-0031): <tetsu-session form-token="…"> wraps the page's session dialog, which the server
// renders. It reads how long the session has left without renewing it, opens the dialog two minutes before a limit,
// renews the session only when the person chooses to stay, keeps every tab of the site in step, and sends a page
// whose session has ended to log in (docs/research/identity/operator-lifecycles.md).

import { call, Refused } from "../shared/http";
import { loggedOut } from "../shared/login";

/** Where a page reads its session's time left (GET), and stays logged in (POST). */
const SESSION = "/ops/session";

/** How many seconds before a limit the dialog opens: HMRC's service timeout pattern warns with two minutes left. */
export const WARN = 120;

/** How many seconds to wait before asking again when the server could not say. */
export const RETRY = 60;

/** The parts of the browser the island uses, passed to `define` so tests can stand in for them. `channel` reaches
 * the site's other tabs, where the browser has one. */
export interface Browser {
  location: Pick<Location, "pathname" | "assign">;
  page: Pick<Document, "visibilityState" | "addEventListener">;
  window: Pick<Window, "addEventListener">;
  channel: Pick<BroadcastChannel, "postMessage" | "addEventListener"> | undefined;
}

/** Which limit is nearer: the idle one, which staying renews, or the absolute one, which only a new login does. */
type Limit = "idle" | "absolute";

/** How a time left reads in the dialog: in whole minutes, then in seconds in the last one. */
export function spoken(seconds: number): string {
  if (seconds >= 60) {
    const minutes = Math.ceil(seconds / 60);
    return minutes === 1 ? "1 minute" : `${minutes} minutes`;
  }
  return seconds === 1 ? "1 second" : `${seconds} seconds`;
}

/** The parts of a `<tetsu-session>` it works through, if it has them all. */
interface Parts {
  dialog: HTMLDialogElement;
  stay: HTMLButtonElement;
  left: Element;
}

function partsOf(host: HTMLElement): Parts | undefined {
  const dialog = host.querySelector("dialog");
  const stay = host.querySelector<HTMLButtonElement>("button[data-stay]");
  const left = host.querySelector("[data-left]");
  if (dialog === null || stay === null || left === null) {
    return undefined;
  }
  return { dialog, stay, left };
}

/** The island's clock: one timer at a time, set from what the server last said. */
class Clock {
  private timer: ReturnType<typeof setTimeout> | undefined;
  /** When the nearer limit falls, by this browser's clock, from the server's last answer. */
  private endsAt = 0;

  constructor(
    private readonly host: HTMLElement,
    private readonly parts: Parts,
    private readonly browser: Browser,
  ) {}

  /** Asks the server how long the session has left, which renews nothing, and sets the clock from its answer. */
  readonly check = async (): Promise<void> => {
    try {
      this.set(await call(SESSION));
    } catch (error) {
      this.failed(error);
    }
  };

  /** Stays logged in: a request of the person's own, which renews the session, and tells the site's other tabs. */
  readonly stay = async (): Promise<void> => {
    try {
      const left = await call(SESSION, undefined, { form: this.host.getAttribute("form-token") ?? "" });
      this.browser.channel?.postMessage("renewed");
      this.set(left);
    } catch (error) {
      this.failed(error);
    }
  };

  /** Escape closes the dialog; where it offered to stay, before the idle limit, that stays logged in, as HMRC's
   * does. */
  readonly cancelled = (): void => {
    if (!this.parts.stay.hidden) {
      void this.stay();
    }
  };

  private set(answer: Record<string, unknown>): void {
    const idle = Number(answer.idle);
    const absolute = Number(answer.absolute);
    const left = Math.min(idle, absolute);
    this.endsAt = Date.now() + left * 1000;
    if (left > WARN) {
      this.parts.dialog.close();
      this.after(left - WARN, this.check);
    } else {
      this.warn(idle <= absolute ? "idle" : "absolute");
    }
  }

  /** A session that ended goes to log in; any other failure is asked about again in a minute. */
  private failed(error: unknown): void {
    if (error instanceof Refused && error.status === 401) {
      clearTimeout(this.timer);
      this.browser.channel?.postMessage("ended");
      this.browser.location.assign(loggedOut(this.browser.location.pathname));
      return;
    }
    this.after(RETRY, this.check);
  }

  /** Opens the dialog for the nearer limit, and counts down each second; at the limit it asks the server again. */
  private warn(nearer: Limit): void {
    for (const limit of ["idle", "absolute"] as const) {
      for (const part of this.host.querySelectorAll<HTMLElement>(`[data-${limit}]`)) {
        part.hidden = limit !== nearer;
      }
    }
    if (!this.parts.dialog.open) {
      this.parts.dialog.showModal();
    }
    this.tick();
  }

  private readonly tick = (): void => {
    const left = Math.ceil((this.endsAt - Date.now()) / 1000);
    if (left <= 0) {
      void this.check();
      return;
    }
    this.parts.left.textContent = spoken(left);
    this.after(1, this.tick);
  };

  private after(seconds: number, then: () => void): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(then, seconds * 1000);
  }
}

/** The channel the site's tabs share, where the browser has one, so staying in one tab, or its session ending,
 * reaches the others (HMRC's `data-synchronise-tabs`). */
export function channel(): Browser["channel"] {
  return typeof BroadcastChannel === "function" ? new BroadcastChannel("tetsu-session") : undefined;
}

/** Readies a `<tetsu-session>`, once, if it has its parts: it checks at once, again when another tab says something
 * changed, and when the page is shown again, from another tab or the back-forward cache. */
export function ready(host: HTMLElement, browser: Browser): void {
  const parts = partsOf(host);
  if (host.dataset.ready !== undefined || parts === undefined) {
    return;
  }
  host.dataset.ready = "";
  const clock = new Clock(host, parts, browser);
  parts.stay.addEventListener("click", clock.stay);
  parts.dialog.addEventListener("cancel", clock.cancelled);
  browser.channel?.addEventListener("message", clock.check);
  browser.page.addEventListener("visibilitychange", () => {
    if (browser.page.visibilityState === "visible") {
      void clock.check();
    }
  });
  browser.window.addEventListener("pageshow", (event) => {
    if ((event as PageTransitionEvent).persisted) {
      void clock.check();
    }
  });
  void clock.check();
}

/** Registers the island. The module is deferred, so it is readied as soon as it runs, for the markup already
 * parsed. */
export function define(registry: CustomElementRegistry, browser: Browser): void {
  registry.define(
    "tetsu-session",
    class extends HTMLElement {
      connectedCallback(): void {
        ready(this, browser);
      }
    },
  );
}
