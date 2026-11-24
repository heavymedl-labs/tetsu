// The session island (ADR-0031): <tetsu-session form-token="…"> wraps the page's session dialog, which the server
// renders. It reads how long the session has left without renewing it, opens the dialog two minutes before a limit,
// renews the session only when the person chooses to stay, keeps every tab of the site in step, and sends a page
// whose session has ended to log in (docs/research/identity/operator-lifecycles.md).

import { call, Refused } from "../shared/http";
import { loggedOut } from "../shared/login";
import { Deadline, Timer } from "./timer";

/** Where a page reads its session's time left (GET), and stays logged in (POST). */
const SESSION = "/admin/session";

/** How many seconds before a limit the dialog opens: HMRC's service timeout pattern warns with two minutes left. */
export const WARN = 120;

/** How many seconds apart a visible page asks the server, so an ending no limit foretold, as a disable, is seen
 * within a minute, and how long it waits before asking again when the server could not say. */
export const RETRY = 60;

/** How many seconds a request may take before the island gives up on it, as on a connection a sleep left half open. */
export const TIMEOUT = 10;

/** How many seconds a tab told of an ending waits before asking: the logout that told it may still be on its way. */
export const SETTLE = 2;

/** How many seconds after the dialog opens a dismissal is taken as meant for the page, not for it. */
export const GRACE = 1;

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
  private readonly timer = new Timer();
  /** When the nearer limit falls, from the server's last answer. */
  private deadline = new Deadline(0);
  /** The latest request this page made, so only its answer sets the clock: an older answer may be stale, as a read
   * that left before a stay answered after it. */
  private latest: object = {};
  /** When the dialog last opened, by the wall clock. */
  private opened = Number.NEGATIVE_INFINITY;
  /** Whether another tab told this one the session ended, so it does not pass that on again. */
  private heard = false;

  constructor(
    private readonly host: HTMLElement,
    private readonly parts: Parts,
    private readonly browser: Browser,
  ) {}

  /** Asks the server how long the session has left, which renews nothing, and sets the clock from its answer. A
   * hidden page does not ask, and lets its clock stop: it asks again once shown. */
  readonly check = async (): Promise<void> => {
    if (this.browser.page.visibilityState !== "hidden") {
      await this.ask((signal) => call(SESSION, signal));
    }
  };

  /** Stays logged in: a request of the person's own, which renews the session, and tells the site's other tabs,
   * even if a later request's answer is the one that sets this page's clock. A form token from before a login in
   * another tab is refused, so the page loads afresh for a new one. */
  readonly stay = (): Promise<void> =>
    this.ask(async (signal) => {
      try {
        const left = await call(SESSION, signal, { form: this.host.getAttribute("form-token") ?? "" });
        this.browser.channel?.postMessage("renewed");
        return left;
      } catch (error) {
        if (error instanceof Refused && error.status === 403) {
          this.browser.location.assign(this.browser.location.pathname);
        }
        throw error;
      }
    });

  /** Another tab says something changed. One that ended the session may still be logging out, so an ending is asked
   * about after a moment. */
  readonly told = (message: MessageEvent): void => {
    if (message.data === "ended") {
      this.heard = true;
      this.timer.after(SETTLE, this.check);
      return;
    }
    void this.check();
  };

  /** Closing the dialog, by Escape or a phone's back gesture, is the person dismissing it, and where it offered to
   * stay, before the idle limit, that stays logged in, as HMRC's does. A dismissal as it opens was meant for the page,
   * so the dialog stays open. */
  readonly cancelled = (event: Event): void => {
    if (Date.now() - this.opened < GRACE * 1000) {
      // A browser may not let the page refuse a dismissal; then the dialog opens again once it has closed.
      if (event.cancelable) {
        event.preventDefault();
      } else {
        this.parts.dialog.addEventListener("close", () => this.parts.dialog.showModal(), { once: true });
      }
      return;
    }
    if (!this.parts.stay.hidden) {
      void this.stay();
    }
  };

  /** Makes a request, given up after a while, with the clock still running should it never answer. */
  private async ask(request: (signal: AbortSignal) => Promise<Record<string, unknown>>): Promise<void> {
    const asked = {};
    this.latest = asked;
    this.retry();
    const giving = new AbortController();
    const limit = setTimeout(() => giving.abort(), TIMEOUT * 1000);
    try {
      const answer = await request(giving.signal);
      if (asked === this.latest) {
        this.set(answer);
      }
    } catch (error) {
      if (asked === this.latest) {
        this.failed(error);
      }
    } finally {
      clearTimeout(limit);
    }
  }

  /** Sets the clock from an answer. One that is not, as a captive portal's or a waking host's page answering 200
   * with no times, leaves the retry set when it was asked. */
  private set(answer: Record<string, unknown>): void {
    const { idle, absolute } = answer;
    if (typeof idle !== "number" || typeof absolute !== "number") {
      return;
    }
    const left = Math.min(idle, absolute);
    this.deadline = new Deadline(left);
    if (left > WARN) {
      this.parts.dialog.close();
      this.timer.after(Math.min(left - WARN, RETRY), this.check);
    } else {
      this.warn(idle <= absolute ? "idle" : "absolute");
    }
  }

  /** A session that ended goes to log in. Anything else leaves the retry set when it was asked. */
  private failed(error: unknown): void {
    if (error instanceof Refused && error.status === 401) {
      this.timer.stop();
      if (!this.heard) {
        this.browser.channel?.postMessage("ended");
      }
      this.browser.location.assign(loggedOut(this.browser.location.pathname));
    }
  }

  /** Keeps the clock running until an answer sets it, should none come: the countdown runs on while the dialog is
   * open and its limit is still ahead, and otherwise the island asks again in a minute. */
  private retry(): void {
    if (this.parts.dialog.open && this.deadline.left() > 0) {
      this.tick();
    } else {
      this.timer.after(RETRY, this.check);
    }
  }

  /** Opens the dialog for the nearer limit, and counts down each second. */
  private warn(nearer: Limit): void {
    for (const limit of ["idle", "absolute"] as const) {
      for (const part of this.host.querySelectorAll<HTMLElement>(`[data-${limit}]`)) {
        part.hidden = limit !== nearer;
      }
    }
    if (!this.parts.dialog.open) {
      this.opened = Date.now();
      this.parts.dialog.showModal();
    }
    this.tick();
  }

  /** Counts down each second; at the limit it waits a second more and asks again, so a session the server still
   * holds for under a second is not asked about without pause. */
  private readonly tick = (): void => {
    const left = Math.ceil(this.deadline.left() / 1000);
    if (left <= 0) {
      this.timer.after(1, this.check);
      return;
    }
    this.parts.left.textContent = spoken(left);
    this.timer.after(1, this.tick);
  };
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
  parts.logout.setAttribute("action", `/admin/logout?next=${browser.location.pathname}`);
  const clock = new Clock(host, parts, browser);
  parts.stay.addEventListener("click", clock.stay);
  parts.dialog.addEventListener("cancel", clock.cancelled);
  browser.channel?.addEventListener("message", clock.told);
  browser.window.addEventListener("online", clock.check);
  // Logging out from any form on the page ends the session in every tab, so the others leave at once.
  browser.page.addEventListener("submit", (event) => {
    const form = event.target;
    if (form instanceof HTMLFormElement && form.getAttribute("action")?.startsWith("/admin/logout")) {
      browser.channel?.postMessage("ended");
    }
  });
  browser.page.addEventListener("visibilitychange", clock.check);
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
