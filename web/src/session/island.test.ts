import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { call, Refused } from "../shared/http";
import { type Browser, channel, define, GRACE, RETRY, ready, SETTLE, spoken, TIMEOUT, WARN } from "./island";
import { STEP } from "./timer";

vi.mock("../shared/http", async (original) => ({
  ...(await original<typeof import("../shared/http")>()),
  call: vi.fn(),
}));

const asked = vi.mocked(call);

/** A browser whose page is visible, with listeners kept so a test can fire them. */
function browser() {
  const listeners: Record<string, (event: unknown) => void> = {};
  const listen = (name: string, listener: (event: unknown) => void) => {
    listeners[name] = listener;
  };
  const page = {
    location: { pathname: "/ops", assign: vi.fn() },
    page: { visibilityState: "visible" as DocumentVisibilityState, addEventListener: vi.fn(listen) },
    window: { addEventListener: vi.fn(listen) },
    channel: { postMessage: vi.fn(), addEventListener: vi.fn(listen) },
  };
  return { page: page as unknown as Browser & typeof page, listeners };
}

/** A `<tetsu-session>` as the server renders it, with a dialog that opens and closes as a browser's does. */
function island(inside = PARTS): HTMLElement {
  const document = window.document.implementation.createHTMLDocument("test");
  document.body.innerHTML = `<tetsu-session form-token="F1">${inside}</tetsu-session>`;
  const dialog = document.querySelector("dialog");
  if (dialog !== null) {
    dialog.showModal = vi.fn(() => dialog.setAttribute("open", ""));
    dialog.close = vi.fn(() => dialog.removeAttribute("open"));
  }
  return document.querySelector("tetsu-session") as HTMLElement;
}

const PARTS =
  "<dialog><span data-idle>Idle</span><span data-absolute hidden>Absolute</span><span data-left></span>" +
  '<button type="button" data-idle data-stay>Stay</button><form method="post" action="/ops/logout"></form></dialog>';

const dialog = (host: HTMLElement) => host.querySelector("dialog") as HTMLDialogElement;
const left = (host: HTMLElement) => host.querySelector("[data-left]")?.textContent;
const shown = (host: HTMLElement) =>
  [...host.querySelectorAll<HTMLElement>("[data-idle], [data-absolute]")].filter((part) => !part.hidden)
    .length;

/** Has the server answer each read or stay with these seconds to each limit, in turn. */
function answers(...lefts: [number, number][]) {
  for (const [idle, absolute] of lefts) {
    asked.mockResolvedValueOnce({ idle, absolute });
  }
}

/** Lets the island's requests settle and moves the clock on `seconds`. */
async function elapse(seconds: number) {
  await vi.advanceTimersByTimeAsync(seconds * 1000);
}

beforeEach(() => {
  vi.useFakeTimers();
  asked.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("spoken", () => {
  it("says whole minutes, then seconds in the last minute", () => {
    expect([121, 120, 61, 60, 59, 1].map(spoken)).toEqual([
      "3 minutes",
      "2 minutes",
      "2 minutes",
      "1 minute",
      "59 seconds",
      "1 second",
    ]);
  });
});

describe("a session island's clock", () => {
  it("reads the time left at once, which renews nothing, again each minute, and warns two minutes before", async () => {
    const { page } = browser();
    const host = island();
    answers([WARN + 180, 28_800], [WARN + 120, 28_740], [WARN + 60, 28_680], [WARN, 28_620]);
    ready(host, page);
    await elapse(0);
    expect(asked).toHaveBeenCalledWith("/ops/session", expect.any(AbortSignal));
    await elapse(3 * RETRY - 1);
    expect([asked.mock.calls.length, dialog(host).open]).toEqual([3, false]);
    await elapse(1);
    expect([asked.mock.calls.length, dialog(host).open, left(host), shown(host)]).toEqual([
      4,
      true,
      "2 minutes",
      2,
    ]);
  });
});

describe("a session island's countdown", () => {
  it("counts down each second, then asks again at the limit, and goes to log in once it has ended", async () => {
    const { page } = browser();
    const host = island();
    answers([61, 28_000]);
    asked.mockRejectedValueOnce(new Refused("you are logged out; log in again", 401));
    ready(host, page);
    await elapse(2);
    expect(left(host)).toBe("59 seconds");
    await elapse(58);
    expect(left(host)).toBe("1 second");
    await elapse(1);
    // At the limit it waits a second more before asking, so a session held for under a second is not asked about
    // without pause.
    expect([page.location.assign.mock.calls.length, asked.mock.calls.length]).toEqual([0, 1]);
    await elapse(1);
    expect(page.location.assign).toHaveBeenCalledWith("/ops/login?ended=1");
    expect(page.channel.postMessage).toHaveBeenCalledWith("ended");
  });

  it("keeps the dialog open, once open, as it counts down and when asked again", async () => {
    const { page, listeners } = browser();
    const host = island();
    answers([WARN, 28_000], [WARN - 10, 27_990]);
    ready(host, page);
    await elapse(3);
    listeners.message?.(new MessageEvent("message", { data: "renewed" }));
    await elapse(0);
    expect([asked.mock.calls.length, dialog(host).open]).toEqual([2, true]);
    expect(dialog(host).showModal).toHaveBeenCalledTimes(1);
  });

  it("keeps one timer, so asking again early never asks twice later", async () => {
    const { page, listeners } = browser();
    asked.mockResolvedValue({ idle: 900, absolute: 28_800 });
    ready(island(), page);
    await elapse(0);
    listeners.message?.(new MessageEvent("message", { data: "renewed" }));
    await elapse(RETRY);
    expect(asked).toHaveBeenCalledTimes(3);
  });
});

describe("a session island when the server cannot say", () => {
  it("asks again in a minute when the server cannot say", async () => {
    const { page } = browser();
    const host = island();
    asked.mockRejectedValueOnce(
      new Refused("logging in is unavailable just now; try again in a minute", 503),
    );
    answers([900, 28_800]);
    ready(host, page);
    await elapse(RETRY - 1);
    expect(asked).toHaveBeenCalledTimes(1);
    await elapse(1);
    expect([asked.mock.calls.length, page.location.assign.mock.calls.length]).toEqual([2, 0]);
  });
});

describe("a session island's dialog", () => {
  it("stays logged in when asked, with the page's form token, and tells the other tabs", async () => {
    const { page } = browser();
    const host = island();
    answers([WARN, 28_000], [900, 27_900]);
    ready(host, page);
    await elapse(0);
    (host.querySelector("[data-stay]") as HTMLButtonElement).click();
    await elapse(0);
    expect(asked).toHaveBeenLastCalledWith("/ops/session", expect.any(AbortSignal), { form: "F1" });
    expect([dialog(host).open, page.channel.postMessage.mock.calls]).toEqual([
      false,
      [["renewed"], ["renewed"]],
    ]);
  });

  it("sends nothing for a form token the page lacks, and asks again in a minute when staying is refused", async () => {
    const { page } = browser();
    const host = island();
    host.removeAttribute("form-token");
    answers([WARN + 30, 28_000]);
    ready(host, page);
    await elapse(30);
    asked.mockRejectedValueOnce(new Refused("this request was refused", 403));
    answers([900, 27_900]);
    (host.querySelector("[data-stay]") as HTMLButtonElement).click();
    await elapse(0);
    expect(asked).toHaveBeenLastCalledWith("/ops/session", expect.any(AbortSignal), { form: "" });
    expect(page.channel.postMessage.mock.calls).toEqual([["renewed"]]);
    await elapse(RETRY);
    expect([asked.mock.calls.length, dialog(host).open]).toEqual([4, false]);
  });

  it("goes to log in, once, when staying finds the session ended, and counts down no more", async () => {
    const { page } = browser();
    const host = island();
    answers([WARN, 28_000]);
    asked.mockRejectedValue(new Refused("you are logged out; log in again", 401));
    ready(host, page);
    await elapse(0);
    (host.querySelector("[data-stay]") as HTMLButtonElement).click();
    await elapse(WARN + 5);
    expect([page.location.assign.mock.calls.length, asked.mock.calls.length]).toEqual([1, 2]);
  });
});

describe("a session island's dialog near the 8 hour limit", () => {
  it("warns of the idle limit when both fall together, since staying renews it", async () => {
    const { page } = browser();
    const host = island();
    answers([100, 100]);
    ready(host, page);
    await elapse(0);
    expect((host.querySelector("[data-stay]") as HTMLElement).hidden).toBe(false);
  });

  it("warns of the 8 hour limit, which staying cannot renew, with its own words and no way to stay", async () => {
    const { page } = browser();
    const host = island();
    answers([900, 100]);
    ready(host, page);
    await elapse(0);
    const visible = [...host.querySelectorAll<HTMLElement>("[data-idle], [data-absolute]")].filter(
      (part) => !part.hidden,
    );
    expect(visible.map((part) => part.textContent)).toEqual(["Absolute"]);
    expect(left(host)).toBe("2 minutes");
  });

  it("stays logged in on Escape before the idle limit, and only closes before the absolute one", async () => {
    for (const [lefts, stays] of [
      [[WARN, 28_000] as [number, number], 1],
      [[900, 100] as [number, number], 0],
    ] as const) {
      asked.mockReset();
      const { page } = browser();
      const host = island();
      answers(lefts, [900, 27_900]);
      ready(host, page);
      await elapse(GRACE);
      dialog(host).dispatchEvent(new Event("cancel"));
      await elapse(0);
      expect(asked.mock.calls.filter((made) => made.length === 3)).toHaveLength(stays);
    }
  });
});

describe("a session island among tabs", () => {
  it("asks again when another tab says something changed, closing a dialog another tab answered", async () => {
    const { page, listeners } = browser();
    const host = island();
    answers([WARN, 28_000], [900, 27_900]);
    ready(host, page);
    await elapse(0);
    listeners.message?.(new MessageEvent("message", { data: "renewed" }));
    await elapse(0);
    expect([asked.mock.calls.length, dialog(host).open]).toEqual([2, false]);
  });

  it("asks again when the page is shown, but not when it is hidden", async () => {
    const { page, listeners } = browser();
    answers([900, 28_800], [900, 28_800]);
    ready(island(), page);
    await elapse(0);
    page.page.visibilityState = "hidden";
    listeners.visibilitychange?.(new Event("visibilitychange"));
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(1);
    page.page.visibilityState = "visible";
    listeners.visibilitychange?.(new Event("visibilitychange"));
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(2);
  });

  it("asks again when the back-forward cache restores the page, and not on a fresh load", async () => {
    const { page, listeners } = browser();
    answers([900, 28_800], [900, 28_800]);
    ready(island(), page);
    await elapse(0);
    listeners.pageshow?.(Object.assign(new Event("pageshow"), { persisted: false }));
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(1);
    listeners.pageshow?.(Object.assign(new Event("pageshow"), { persisted: true }));
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(2);
  });
});

describe("a session island alone", () => {
  it("works alone in a browser with no channel to other tabs, staying and ending alike", async () => {
    const { page } = browser();
    const alone = { ...page, channel: undefined };
    const host = island();
    answers([WARN, 28_000], [900, 27_900]);
    ready(host, alone);
    await elapse(0);
    (host.querySelector("[data-stay]") as HTMLButtonElement).click();
    await elapse(0);
    expect([dialog(host).open, asked.mock.calls.length]).toEqual([false, 2]);
    asked.mockRejectedValueOnce(new Refused("you are logged out; log in again", 401));
    await elapse(900 - WARN);
    expect(page.location.assign).toHaveBeenCalledWith("/ops/login?ended=1");
  });
});

describe("readying a session island", () => {
  it("has its dialog's logout return to this page after the next login", () => {
    const { page } = browser();
    const host = island();
    answers([900, 28_800]);
    ready(host, page);
    expect(host.querySelector("form")?.getAttribute("action")).toBe("/ops/logout?next=/ops");
  });

  it("readies it once, however often it is connected", async () => {
    const { page } = browser();
    const host = island();
    answers([900, 28_800]);
    ready(host, page);
    ready(host, page);
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(1);
  });

  it("leaves one without its parts alone", async () => {
    for (const inside of [
      "<span data-left></span><button data-stay></button><form></form>",
      "<dialog><span data-left></span><form></form></dialog>",
      "<dialog><button data-stay></button><form></form></dialog>",
      "<dialog><span data-left></span><button data-stay></button></dialog>",
    ]) {
      const { page } = browser();
      const host = island(inside);
      ready(host, page);
      await elapse(0);
      expect([asked.mock.calls.length, host.dataset.ready]).toEqual([0, undefined]);
    }
  });

  it("registers <tetsu-session>, readying each one connected", async () => {
    const { page } = browser();
    const registry = window.customElements;
    define(registry, page);
    answers([900, 28_800]);
    const host = document.createElement("tetsu-session");
    host.innerHTML = PARTS;
    document.body.append(host);
    await elapse(0);
    expect([asked.mock.calls.length, host.dataset.ready]).toEqual([1, ""]);
    host.remove();
  });
});

describe("channel", () => {
  it("opens the channel the site's tabs share, or none in a browser without one", () => {
    const opened = vi.fn();
    vi.stubGlobal(
      "BroadcastChannel",
      class {
        constructor(name: string) {
          opened(name);
        }
      },
    );
    expect([channel() !== undefined, opened.mock.calls]).toEqual([true, [["tetsu-session"]]]);
    vi.stubGlobal("BroadcastChannel", undefined);
    expect(channel()).toBeUndefined();
    vi.unstubAllGlobals();
  });
});

describe("a session island when the operator logs out", () => {
  it("tells the site's other tabs when a logout form on the page is sent, and nothing for other forms", async () => {
    const { page, listeners } = browser();
    answers([900, 28_800]);
    ready(island(), page);
    await elapse(0);
    const form = (action: string | null) => {
      const made = document.createElement("form");
      if (action !== null) {
        made.setAttribute("action", action);
      }
      return { target: made };
    };
    for (const sent of [form("/ops/passkeys"), form(null), { target: document.createElement("div") }]) {
      listeners.submit?.(sent);
    }
    expect(page.channel.postMessage.mock.calls).toEqual([["renewed"]]);
    for (const sent of [form("/ops/logout"), form("/ops/logout?next=/ops")]) {
      listeners.submit?.(sent);
    }
    expect(page.channel.postMessage.mock.calls).toEqual([["renewed"], ["ended"], ["ended"]]);
  });

  it("logs out alone in a browser with no channel to other tabs", () => {
    const { page, listeners } = browser();
    ready(island(), { ...page, channel: undefined });
    const made = document.createElement("form");
    made.setAttribute("action", "/ops/logout");
    expect(() => listeners.submit?.({ target: made })).not.toThrow();
  });
});

describe("a session island's doubts", () => {
  it("tells the site's other tabs when its page loads, since the load renewed the session", () => {
    const { page } = browser();
    answers([900, 28_800]);
    ready(island(), page);
    expect(page.channel.postMessage.mock.calls).toEqual([["renewed"]]);
  });

  it.each([{}, { idle: "50", absolute: 28_800 }, { idle: 900 }])(
    "asks again in a minute when a success carries no times, as a captive portal's page may: %o",
    async (answer) => {
      const { page } = browser();
      const host = island();
      asked.mockResolvedValueOnce(answer);
      answers([900, 28_800]);
      ready(host, page);
      await elapse(RETRY - 1);
      expect([dialog(host).open, asked.mock.calls.length]).toEqual([false, 1]);
      await elapse(1);
      expect(asked).toHaveBeenCalledTimes(2);
    },
  );

  it("sees time that passed while the tab slept, within a step of waking, without asking more often", async () => {
    const { page } = browser();
    answers([900, 28_800]);
    asked.mockRejectedValueOnce(new Refused("you are logged out; log in again", 401));
    ready(island(), page);
    await elapse(STEP - 1);
    expect(asked).toHaveBeenCalledTimes(1);
    // The machine sleeps for an hour; its timers did not run, but its clock moved on.
    vi.setSystemTime(Date.now() + 3_600_000);
    await elapse(1);
    expect(page.location.assign).toHaveBeenCalledWith("/ops/login?ended=1");
  });

  it("asks again when the browser is back online", async () => {
    const { page, listeners } = browser();
    answers([900, 28_800], [900, 28_800]);
    ready(island(), page);
    await elapse(0);
    listeners.online?.(new Event("online"));
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(2);
  });
});

describe("a session island's answers out of order", () => {
  /** Has the next request wait until the test lets it answer, or fail. */
  function held() {
    const hold: { answer?: (left: object) => void; fail?: (error: unknown) => void } = {};
    asked.mockImplementationOnce(
      () =>
        new Promise((answer, fail) => {
          hold.answer = answer as (left: object) => void;
          hold.fail = fail;
        }),
    );
    return hold;
  }

  it("lets only the latest request set the clock, so a stale read never reopens a dialog a stay closed", async () => {
    const { page, listeners } = browser();
    const host = island();
    answers([WARN, 28_000]);
    ready(host, page);
    await elapse(0);
    const read = held();
    listeners.visibilitychange?.(new Event("visibilitychange"));
    answers([900, 27_900]);
    (host.querySelector("[data-stay]") as HTMLButtonElement).click();
    await elapse(0);
    expect(dialog(host).open).toBe(false);
    read.answer?.({ idle: 100, absolute: 27_900 });
    await elapse(0);
    expect(dialog(host).open).toBe(false);
  });

  it("ignores a stale request's failure too", async () => {
    const { page, listeners } = browser();
    const host = island();
    answers([900, 28_800]);
    ready(host, page);
    await elapse(0);
    const read = held();
    listeners.visibilitychange?.(new Event("visibilitychange"));
    answers([900, 28_800]);
    listeners.online?.(new Event("online"));
    await elapse(0);
    read.fail?.(new Refused("you are logged out; log in again", 401));
    await elapse(0);
    expect(page.location.assign).not.toHaveBeenCalled();
  });
});

describe("a session island's second doubts", () => {
  it("ignores a dismissal in the dialog's first second, as meant for the page, keeping it open", async () => {
    const { page } = browser();
    const host = island();
    answers([WARN, 28_000]);
    ready(host, page);
    await elapse(0);
    const dismissal = new Event("cancel", { cancelable: true });
    dialog(host).dispatchEvent(dismissal);
    await elapse(0);
    expect([dismissal.defaultPrevented, asked.mock.calls.length]).toEqual([true, 1]);
  });

  it("asks about an ending another tab reports after a moment, and does not pass it on", async () => {
    const { page, listeners } = browser();
    answers([900, 28_800]);
    asked.mockRejectedValueOnce(new Refused("you are logged out; log in again", 401));
    ready(island(), page);
    await elapse(0);
    page.channel.postMessage.mockClear();
    listeners.message?.(new MessageEvent("message", { data: "ended" }));
    await elapse(SETTLE - 1);
    expect(asked).toHaveBeenCalledTimes(1);
    await elapse(1);
    expect(page.location.assign).toHaveBeenCalledWith("/ops/login?ended=1");
    expect(page.channel.postMessage).not.toHaveBeenCalled();
  });
});

describe("a session island whose stay or read fails", () => {
  it("loads the page afresh when staying is refused for a form token from before another login", async () => {
    const { page } = browser();
    const host = island();
    answers([WARN, 28_000]);
    asked.mockRejectedValueOnce(new Refused("this request was refused", 403));
    ready(host, page);
    await elapse(0);
    (host.querySelector("[data-stay]") as HTMLButtonElement).click();
    await elapse(0);
    expect(page.location.assign).toHaveBeenCalledWith("/ops");
  });

  it("keeps counting down when a request fails while the dialog is open", async () => {
    const { page, listeners } = browser();
    const host = island();
    answers([WARN, 28_000]);
    asked.mockRejectedValueOnce(
      new Refused("logging in is unavailable just now; try again in a minute", 503),
    );
    ready(host, page);
    await elapse(0);
    listeners.message?.(new MessageEvent("message", { data: "renewed" }));
    await elapse(0);
    await elapse(61);
    expect([dialog(host).open, left(host)]).toEqual([true, "59 seconds"]);
  });
});

describe("a session island whose request never answers", () => {
  it("still asks again in a minute", async () => {
    const { page } = browser();
    asked.mockImplementationOnce(() => new Promise(() => undefined));
    answers([900, 28_800]);
    ready(island(), page);
    await elapse(RETRY - 1);
    expect(asked).toHaveBeenCalledTimes(1);
    await elapse(1);
    expect(asked).toHaveBeenCalledTimes(2);
  });

  it("waits a minute, not a second, before asking again at the limit", async () => {
    const { page } = browser();
    answers([WARN, 28_000]);
    asked.mockImplementation(() => new Promise(() => undefined));
    ready(island(), page);
    await elapse(WARN + 1);
    expect(asked).toHaveBeenCalledTimes(2);
    await elapse(RETRY - 1);
    expect(asked).toHaveBeenCalledTimes(2);
    await elapse(1);
    expect(asked).toHaveBeenCalledTimes(3);
  });
});

describe("a session island's countdown, at its edges", () => {
  it("ignores a dismissal half a second in, and stays logged in on one after the grace", async () => {
    const { page } = browser();
    const host = island();
    answers([WARN, 28_000], [900, 27_900]);
    ready(host, page);
    await vi.advanceTimersByTimeAsync((GRACE * 1000) / 2);
    dialog(host).dispatchEvent(new Event("cancel", { cancelable: true }));
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync((GRACE * 1000) / 2);
    dialog(host).dispatchEvent(new Event("cancel", { cancelable: true }));
    await elapse(0);
    expect(asked).toHaveBeenLastCalledWith("/ops/session", expect.any(AbortSignal), { form: "F1" });
  });

  it("keeps counting down when an ending another tab reports is asked about and never answered", async () => {
    const { page, listeners } = browser();
    const host = island();
    answers([WARN, 28_000]);
    ready(host, page);
    await elapse(0);
    asked.mockImplementationOnce(() => new Promise(() => undefined));
    listeners.message?.(new MessageEvent("message", { data: "ended" }));
    await elapse(SETTLE);
    await elapse(60);
    expect(left(host)).toBe("58 seconds");
  });

  it("asks again in a minute, not a second, for a request made at the very limit that never answers", async () => {
    const { page, listeners } = browser();
    answers([WARN, 28_000]);
    ready(island(), page);
    await elapse(WARN);
    asked.mockImplementation(() => new Promise(() => undefined));
    listeners.message?.(new MessageEvent("message", { data: "renewed" }));
    await elapse(0);
    const made = asked.mock.calls.length;
    await elapse(1);
    expect(asked).toHaveBeenCalledTimes(made);
  });
});

describe("a session island's third doubts", () => {
  it("does not ask while its page is hidden, and asks once it is shown", async () => {
    const { page, listeners } = browser();
    page.page.visibilityState = "hidden";
    answers([900, 28_800]);
    ready(island(), page);
    await elapse(RETRY * 3);
    expect(asked).not.toHaveBeenCalled();
    page.page.visibilityState = "visible";
    listeners.visibilitychange?.(new Event("visibilitychange"));
    await elapse(0);
    expect(asked).toHaveBeenCalledTimes(1);
  });

  it("gives up on a request after its timeout, as on a connection a sleep left half open", async () => {
    const { page } = browser();
    let signal: AbortSignal | undefined;
    asked.mockImplementationOnce((_path, given) => {
      signal = given;
      return new Promise(() => undefined);
    });
    ready(island(), page);
    await elapse(TIMEOUT - 1);
    expect(signal?.aborted).toBe(false);
    await elapse(1);
    expect(signal?.aborted).toBe(true);
  });

  it("stops the timeout once the request answers", async () => {
    const { page } = browser();
    let signal: AbortSignal | undefined;
    asked.mockImplementationOnce(async (_path, given) => {
      signal = given;
      return { idle: 900, absolute: 28_800 };
    });
    ready(island(), page);
    await elapse(TIMEOUT);
    expect(signal?.aborted).toBe(false);
  });
});

describe("a session island's third doubts, refusals and dismissals", () => {
  it("never loads the page afresh for a refused read, only for a refused stay", async () => {
    const { page } = browser();
    asked.mockRejectedValueOnce(new Refused("this request was refused", 403));
    ready(island(), page);
    await elapse(0);
    expect(page.location.assign).not.toHaveBeenCalled();
  });

  it("opens the dialog again when a browser closes it on a dismissal it would not let the page refuse", async () => {
    const { page } = browser();
    const host = island();
    answers([WARN, 28_000]);
    ready(host, page);
    await elapse(0);
    dialog(host).dispatchEvent(new Event("cancel", { cancelable: false }));
    dialog(host).close();
    dialog(host).dispatchEvent(new Event("close"));
    expect([dialog(host).open, asked.mock.calls.length]).toEqual([true, 1]);
    // Only that once: a later close, as staying brings, stays closed.
    answers([900, 27_900]);
    (host.querySelector("[data-stay]") as HTMLButtonElement).click();
    await elapse(0);
    dialog(host).dispatchEvent(new Event("close"));
    expect(dialog(host).open).toBe(false);
  });
});
