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

/** The longest one timer runs: a long wait is taken in steps, each measured against the wall clock, so a tab that
 * slept, whose timers may not have run, sees the time that passed within a step of waking. */
export const STEP = 60;

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
  logout: HTMLFormElement;
}

function partsOf(host: HTMLElement): Parts | undefined {
  const dialog = host.querySelector("dialog");
  const stay = host.querySelector<HTMLButtonElement>("button[data-stay]");
  const left = host.querySelector("[data-left]");
  const logout = host.querySelector("form");
  if (dialog === null || stay === null || left === null || logout === null) {
    return undefined;
  }
  return { dialog, stay, left, logout };
}

/** The island's clock: one timer at a time, set from what the server last said. */
class Clock {
  private timer: ReturnType<typeof setTimeout> | undefined;
  /** When the nearer limit falls, by this browser's clock, from the server's last answer. */
  private endsAt = 0;
  /** The latest request this page made, so only its answer sets the clock: an older answer may be stale, as a read
   * that left before a stay answered after it. */
  private latest: object = {};

  constructor(
    private readonly host: HTMLElement,
    private readonly parts: Parts,
    private readonly browser: Browser,
  ) {}

  /** Asks the server how long the session has left, which renews nothing, and sets the clock from its answer. */
  readonly check = (): Promise<void> => this.ask(() => call(SESSION));

  /** Stays logged in: a request of the person's own, which renews the session, and tells the site's other tabs,
   * even if a later request's answer is the one that sets this page's clock. */
  readonly stay = (): Promise<void> =>
    this.ask(async () => {
      const left = await call(SESSION, undefined, { form: this.host.getAttribute("form-token") ?? "" });
      this.browser.channel?.postMessage("renewed");
      return left;
    });

  /** Closing the dialog, by Escape or a phone's back gesture, is the person dismissing it; where it offered to stay,
   * before the idle limit, that stays logged in, as HMRC's does. */
  readonly cancelled = (): void => {
    if (!this.parts.stay.hidden) {
      void this.stay();
    }
  };

  private async ask(request: () => Promise<Record<string, unknown>>): Promise<void> {
    const asked = {};
    this.latest = asked;
    try {
      const answer = await request();
      if (asked === this.latest) {
        this.set(answer);
      }
    } catch (error) {
      if (asked === this.latest) {
        this.failed(error);
      }
    }
  }

  /** Sets the clock from an answer, or asks again later when it is not one: a captive portal's or a waking host's
   * page may answer 200 with no times at all. */
  private set(answer: Record<string, unknown>): void {
    const { idle, absolute } = answer;
    if (typeof idle !== "number" || typeof absolute !== "number") {
      this.after(RETRY, this.check);
      return;
    }
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

  /** Counts down each second; at the limit it waits a second more and asks again, so a session the server still
   * holds for under a second is not asked about without pause. */
  private readonly tick = (): void => {
    const left = Math.ceil((this.endsAt - Date.now()) / 1000);
    if (left <= 0) {
      this.after(1, this.check);
      return;
    }
    this.parts.left.textContent = spoken(left);
    this.after(1, this.tick);
  };

  /** Runs `then` once `seconds` have passed by the wall clock, through timers of at most a step each. */
  private after(seconds: number, then: () => void): void {
    clearTimeout(this.timer);
    const due = Date.now() + seconds * 1000;
    const step = (): void => {
      const wait = due - Date.now();
      if (wait <= 0) {
        then();
        return;
      }
      this.timer = setTimeout(step, Math.min(wait, STEP * 1000));
    };
    step();
  }
}

/** The channel the site's tabs share, where the browser has one, so staying in one tab, or its session ending,
 * reaches the others (HMRC's `data-synchronise-tabs`). */
export function channel(): Browser["channel"] {
  return typeof BroadcastChannel === "function" ? new BroadcastChannel("tetsu-session") : undefined;
}

/** Readies a `<tetsu-session>`, once, if it has its parts: it checks at once, again when another tab says something
 * changed, when the browser is back online, and when the page is shown again, from another tab or the back-forward
 * cache. */
export function ready(host: HTMLElement, browser: Browser): void {
  const parts = partsOf(host);
  if (host.dataset.ready !== undefined || parts === undefined) {
    return;
  }
  host.dataset.ready = "";
  // Logging out from the dialog, as "Log in again" before the 8 hour limit does, returns here after the next login.
  parts.logout.setAttribute("action", `/ops/logout?next=${browser.location.pathname}`);
  const clock = new Clock(host, parts, browser);
  parts.stay.addEventListener("click", clock.stay);
  parts.dialog.addEventListener("cancel", clock.cancelled);
  browser.channel?.addEventListener("message", clock.check);
  browser.window.addEventListener("online", clock.check);
  // Logging out from any form on the page ends the session in every tab, so the others leave at once.
  browser.page.addEventListener("submit", (event) => {
    const form = event.target;
    if (form instanceof HTMLFormElement && form.getAttribute("action")?.startsWith("/ops/logout")) {
      browser.channel?.postMessage("ended");
    }
  });
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
  // This page's load renewed the session, so a warning in another tab may be out of date.
  browser.channel?.postMessage("renewed");
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
