import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { call, Refused } from "../shared/http";
import { type Asked, ceremony } from "./ceremony";
import { type Browser, define, ready, said, signal } from "./island";

// The ceremony is ceremony.test.ts's; here it only answers, so each island's handling of it shows.
vi.mock("./ceremony", async (original) => ({
  ...(await original<typeof import("./ceremony")>()),
  ceremony: vi.fn(),
}));
const run = vi.mocked(ceremony);

// Reading a staff invite is the island's one call of its own; here it only answers.
vi.mock("../shared/http", async (original) => ({
  ...(await original<typeof import("../shared/http")>()),
  call: vi.fn(),
}));
const read = vi.mocked(call);

const device = {} as CredentialsContainer;

/** A browser that can use passkeys, at an invite link's address. */
function browser(passkeys: Partial<typeof PublicKeyCredential> = {}): Browser {
  return {
    location: {
      hash: "#L1",
      pathname: "/admin/join",
      search: "?from=mail",
      hostname: "admin.example.com",
      assign: vi.fn(),
    },
    history: { replaceState: vi.fn() },
    credentials: device,
    passkeys: passkeys as typeof PublicKeyCredential,
  };
}

/** A `<tetsu-passkey>` as the server renders it, with `attributes` and `inside` as its content, in the admins' realm
 * unless `attributes` names another. */
function island(
  attributes: string,
  inside = '<button type="button" disabled><span data-label>Go</span></button><p role="status"></p>',
): HTMLElement {
  const document = window.document.implementation.createHTMLDocument("test");
  const realm = attributes.includes("base=") ? "" : 'base="/admin" ';
  document.body.innerHTML = `<tetsu-passkey ${realm}${attributes}>${inside}</tetsu-passkey>`;
  return document.querySelector("tetsu-passkey") as HTMLElement;
}

/** `host` with no realm named at all. */
function withoutBase(host: HTMLElement): HTMLElement {
  host.removeAttribute("base");
  return host;
}

/** A ceremony's arguments as the island passes them, asked `asked`. */
function ran(asked: Asked) {
  return [asked, device, expect.any(AbortSignal), expect.any(Function)] as const;
}

const button = (host: HTMLElement) => host.querySelector("button") as HTMLButtonElement;
const status = (host: HTMLElement) => host.querySelector("[role=status]")?.textContent;

/** Presses the island's button and waits until it has said or done what it does with the ceremony's end. */
async function press(host: HTMLElement): Promise<void> {
  button(host).click();
  await vi.waitFor(() => expect(run).toHaveBeenCalled());
  await new Promise((settled) => setTimeout(settled));
}

/** Has every ceremony from now answer `next`, forgetting what was asked before. */
function answering(next: string): void {
  run.mockReset();
  run.mockResolvedValue(next);
}

/** Has the next ceremony send its last request, then lose the answer, as when the network drops. */
function losingTheAnswer(): void {
  run.mockImplementationOnce(async (_asked, _device, _signal, finishing) => {
    finishing();
    throw new TypeError("Failed to fetch");
  });
}

beforeEach(() => {
  run.mockReset();
});

describe("a passkey island", () => {
  it("logs in when pressed, and goes where the ceremony says", async () => {
    const host = island('action="login"');
    const page = browser();
    ready(host, page);
    expect([button(host).disabled, host.dataset.state]).toEqual([false, "ready"]);
    run.mockResolvedValue("/admin");
    await press(host);
    expect(run).toHaveBeenCalledWith(...ran({ action: "login", base: "/admin", sent: {} }));
    expect(page.location.assign).toHaveBeenCalledWith("/admin");
    expect(page.history.replaceState).not.toHaveBeenCalled();
    expect([status(host), host.dataset.state]).toEqual(["Logged in.", "done"]);
  });
});

describe("a passkey island that joins", () => {
  it("takes an invite link from the address, and clears it from the bar at once", () => {
    const page = browser();
    ready(island('action="join"'), page);
    expect(page.history.replaceState).toHaveBeenCalledWith(null, "", "/admin/join?from=mail");
  });

  it("says a page opened without its invite link cannot join, and keeps the button off", async () => {
    // As after going back to the page, or reloading it: the link left the address bar when it first opened.
    for (const hash of ["", "#"]) {
      const host = island('action="join"');
      const page = browser();
      page.location.hash = hash;
      ready(host, page);
      expect([button(host).disabled, status(host), host.dataset.state]).toEqual([
        true,
        "Open the invite again.",
        "failed",
      ]);
      button(host).click();
      await new Promise((settled) => setTimeout(settled));
      expect(run).not.toHaveBeenCalled();
    }
  });

  it("logs in on joining, with one ceremony and no second prompt (ADR-0069)", async () => {
    const host = island('action="join"');
    const page = browser();
    ready(host, page);
    answering("/admin");
    await press(host);
    expect(run.mock.calls).toEqual([
      [
        { action: "join", base: "/admin", sent: { link: "L1" } },
        device,
        expect.any(AbortSignal),
        expect.any(Function),
      ],
    ]);
    expect([status(host), host.dataset.state, host.querySelector("[data-label]")?.textContent]).toEqual([
      "Passkey created. Logging you in.",
      "done",
      "Go",
    ]);
    expect(page.location.assign).toHaveBeenCalledWith("/admin");
  });
});

/** A staff join page's island, which names the element it fills with what the invite gives, and that element,
 * unless `shown` is false. */
function staffJoin(shown = true): { host: HTMLElement; shows: Element | null } {
  const document = window.document.implementation.createHTMLDocument("test");
  const shows = shown ? '<p id="invitation">Create a passkey to log in.</p>' : "";
  document.body.innerHTML = `${shows}<tetsu-passkey base="/app" action="join" invitation="invitation"><button type="button" disabled><span data-label>Create a passkey</span></button><p role="status"></p></tetsu-passkey>`;
  return {
    host: document.querySelector("tetsu-passkey") as HTMLElement,
    shows: document.getElementById("invitation"),
  };
}

/** Readies a staff join's island, with the invite read as `invitation`, and waits until it is ready or failed. */
async function readied(host: HTMLElement, invitation: Promise<Record<string, unknown>>): Promise<Browser> {
  read.mockReturnValueOnce(invitation);
  const page = browser();
  ready(host, page);
  expect([button(host).disabled, host.dataset.state]).toEqual([true, "waiting"]);
  await vi.waitFor(() => expect(host.dataset.state).not.toBe("waiting"));
  return page;
}

describe("a passkey island that joins a business", () => {
  beforeEach(() => {
    read.mockReset();
  });

  it("reads what the invite gives before offering anything, and asks someone new to create a passkey", async () => {
    const { host, shows } = staffJoin();
    await readied(host, Promise.resolve({ business: "Fern Studio", role: "owner", joined: false }));
    expect(read).toHaveBeenCalledWith("/app/join/invitation", undefined, { link: "L1" });
    expect(shows?.textContent).toBe("Join Fern Studio as owner. Create a passkey to log in.");
    expect([
      button(host).disabled,
      host.dataset.state,
      host.querySelector("[data-label]")?.textContent,
    ]).toEqual([false, "ready", "Create a passkey"]);
    answering("/app/settings/profile");
    await press(host);
    expect(run).toHaveBeenCalledWith(...ran({ action: "join", base: "/app", sent: { link: "L1" } }));
  });

  it("asks someone with a passkey to log in to accept, in one press, sending the link", async () => {
    const { host, shows } = staffJoin();
    const page = await readied(host, Promise.resolve({ business: null, role: "owner", joined: true }));
    expect(shows?.textContent).toBe(
      "You're invited to join the business as owner. Log in with your passkey to accept.",
    );
    expect(host.querySelector("[data-label]")?.textContent).toBe("Log in to accept");
    answering("/app/settings/profile");
    await press(host);
    expect(run).toHaveBeenCalledWith(...ran({ action: "accept", base: "/app", sent: { link: "L1" } }));
    expect([status(host), host.dataset.state]).toEqual(["Logged in. Invite accepted.", "done"]);
    expect(page.location.assign).toHaveBeenCalledWith("/app/settings/profile");
  });
});

describe("a passkey island that joins a business, at the edges", () => {
  beforeEach(() => {
    read.mockReset();
  });

  it("still reads the invite when the page has nowhere to say it", async () => {
    const { host } = staffJoin(false);
    await readied(host, Promise.resolve({ business: "Fern Studio", role: "owner", joined: true }));
    expect([host.dataset.state, host.querySelector("[data-label]")?.textContent]).toEqual([
      "ready",
      "Log in to accept",
    ]);
  });

  it("says an invite that ended cannot be used, and keeps the button off", async () => {
    const { host } = staffJoin();
    await readied(host, Promise.reject(new Refused("this invite has ended")));
    expect([button(host).disabled, status(host), host.dataset.state]).toEqual([
      true,
      "This invite has ended.",
      "failed",
    ]);
  });

  it("offers to read again an invite whose read failed for now, and then goes on", async () => {
    for (const failure of [new Refused("try again in a minute", 503), new TypeError("Failed to fetch")]) {
      read.mockReset();
      run.mockReset();
      const { host } = staffJoin();
      await readied(host, Promise.reject(failure));
      expect([button(host).disabled, host.dataset.state]).toEqual([false, "failed"]);
      read.mockReturnValueOnce(Promise.resolve({ business: "Fern Studio", role: "owner", joined: true }));
      button(host).click();
      await vi.waitFor(() => expect(host.dataset.state).toBe("ready"));
      expect(read).toHaveBeenCalledTimes(2);
      expect(run).not.toHaveBeenCalled();
      expect(host.querySelector("[data-label]")?.textContent).toBe("Log in to accept");
      // Read once it works, the button only logs in to accept from then on.
      answering("/app/settings/profile");
      await press(host);
      expect(read).toHaveBeenCalledTimes(2);
      expect(run).toHaveBeenCalledTimes(1);
    }
  });
});

describe("a passkey island that joins a business, read again", () => {
  beforeEach(() => {
    read.mockReset();
  });

  it("reads no invite for any action but a join", () => {
    const host = island('action="login" invitation="invitation"');
    ready(host, browser());
    expect(read).not.toHaveBeenCalled();
    expect(host.dataset.state).toBe("ready");
  });

  it("is readied once, even when connected again while it reads", async () => {
    const { host } = staffJoin();
    read.mockReturnValueOnce(Promise.resolve({ business: "Fern Studio", role: "owner", joined: false }));
    const page = browser();
    ready(host, page);
    ready(host, page);
    await vi.waitFor(() => expect(host.dataset.state).toBe("ready"));
    expect(read).toHaveBeenCalledTimes(1);
  });
});

describe("a passkey island whose join is refused", () => {
  it("says the browser's refusal, keeps the invite's button, and starts nothing more", async () => {
    const host = island('action="join"');
    const page = browser();
    ready(host, page);
    run.mockRejectedValueOnce(new DOMException("no", "NotAllowedError"));
    await press(host);
    await vi.waitFor(() => expect(host.dataset.state).toBe("failed"));
    expect(status(host)).toBe("No passkey was chosen. Try again.");
    expect(page.location.assign).not.toHaveBeenCalled();
    expect(run).toHaveBeenCalledTimes(1);
    expect(host.querySelector("[data-label]")?.textContent).toBe("Go");
  });
});

describe("a passkey island and what it sends", () => {
  it("sends the page a login returns to, which the server checks again, or nothing the page lacks", async () => {
    for (const [attributes, sent] of [
      ['action="login" next="/admin/passkeys"', { next: "/admin/passkeys" }],
      ['action="login"', {}],
    ] as const) {
      run.mockReset();
      run.mockResolvedValue("/admin/passkeys");
      const host = island(attributes);
      ready(host, browser());
      await press(host);
      expect(run).toHaveBeenCalledWith(...ran({ action: "login", base: "/admin", sent: sent }));
    }
  });

  it("sends the admin's form token when adding a passkey, or none the page lacks", async () => {
    for (const [attributes, form] of [
      ['action="add" form-token="F1"', "F1"],
      ['action="add"', ""],
    ] as const) {
      run.mockReset();
      run.mockResolvedValue("/admin");
      const host = island(attributes);
      ready(host, browser());
      await press(host);
      expect(run).toHaveBeenCalledWith(...ran({ action: "add", base: "/admin", sent: { form } }));
      expect(status(host)).toBe("Passkey added.");
    }
  });
});

describe("a passkey island in its realm", () => {
  it("sends a person whose session ended to log in, to return here after", async () => {
    run.mockRejectedValueOnce(new Refused("you are logged out; log in again", 401));
    const host = island('action="add" form-token="F1"');
    const page = browser();
    page.location.pathname = "/admin";
    ready(host, page);
    await press(host);
    expect(page.location.assign).toHaveBeenCalledWith("/admin/login?ended=1");
    expect([status(host), host.dataset.state]).toEqual(["You are logged out; log in again.", "failed"]);
  });

  it("runs its ceremony in its own realm, and sends a person whose session ended to that realm's login", async () => {
    run.mockRejectedValueOnce(new Refused("you are logged out; log in again", 401));
    const host = island('base="/app" action="add" form-token="F1"');
    const page = browser();
    page.location.pathname = "/app/settings/security";
    ready(host, page);
    await press(host);
    expect(run).toHaveBeenCalledWith(...ran({ action: "add", base: "/app", sent: { form: "F1" } }));
    expect(page.location.assign).toHaveBeenCalledWith("/app/login?ended=1&next=/app/settings/security");
  });
});

describe("a passkey island at work", () => {
  it("says it is waiting on the device while the ceremony runs", async () => {
    const host = island('action="login"');
    ready(host, browser());
    let finish: (next: string) => void = () => undefined;
    run.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    button(host).click();
    expect([status(host), host.dataset.state]).toEqual(["Waiting for your device…", "waiting"]);
    finish("/admin");
    await vi.waitFor(() => expect(host.dataset.state).toBe("done"));
  });
});

describe("a passkey island pressed again", () => {
  it("ends the first ceremony, and only the latest press speaks", async () => {
    const host = island('action="login"');
    const page = browser();
    ready(host, page);
    const signals: AbortSignal[] = [];
    let second: (next: string) => void = () => undefined;
    run
      .mockImplementationOnce((_asked, _device, signal) => {
        signals.push(signal);
        return new Promise((_, refuse) =>
          signal.addEventListener("abort", () => refuse(new DOMException("ended", "AbortError"))),
        );
      })
      .mockImplementationOnce((_asked, _device, signal) => {
        signals.push(signal);
        return new Promise((resolve) => (second = resolve));
      });
    button(host).click();
    button(host).click();
    await vi.waitFor(() => expect(signals).toHaveLength(2));
    await new Promise((settled) => setTimeout(settled));
    expect(signals.map((signal) => signal.aborted)).toEqual([true, false]);
    expect([status(host), host.dataset.state]).toEqual(["Waiting for your device…", "waiting"]);
    second("/admin");
    await vi.waitFor(() => expect(host.dataset.state).toBe("done"));
    expect(button(host).disabled).toBe(true);
    expect(page.location.assign).toHaveBeenCalledTimes(1);
  });
});

describe("a passkey island whose join the server refuses or a later press ends", () => {
  it("says the server's refusal of a join in its words", async () => {
    const host = island('action="join"');
    ready(host, browser());
    run.mockRejectedValueOnce(new Refused("this invite has ended. Log in, or ask an admin for a new one"));
    await press(host);
    await vi.waitFor(() => expect(host.dataset.state).toBe("failed"));
    expect(status(host)).toBe("This invite has ended. Log in, or ask an admin for a new one.");
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("says nothing of a join another press ended", async () => {
    const host = island('action="join"');
    ready(host, browser());
    let first: AbortSignal | undefined;
    run.mockImplementationOnce((_asked, _device, signal) => {
      first = signal;
      return new Promise((_, refuse) =>
        signal.addEventListener("abort", () => refuse(new DOMException("ended", "AbortError"))),
      );
    });
    run.mockImplementationOnce(() => new Promise(() => undefined));
    button(host).click();
    await vi.waitFor(() => expect(first).toBeDefined());
    button(host).click();
    await new Promise((settled) => setTimeout(settled));
    expect([status(host), host.dataset.state]).toEqual(["Waiting for your device…", "waiting"]);
    expect(run.mock.calls.map(([asked]) => asked.action)).toEqual(["join", "join"]);
  });
});

describe("a passkey island whose join answer was lost", () => {
  it("turns to logging in, since the passkey may be kept and the link spent", async () => {
    const host = island('action="join"');
    ready(host, browser());
    losingTheAnswer();
    await press(host);
    await vi.waitFor(() => expect(host.dataset.state).toBe("ready"));
    expect([status(host), host.querySelector("[data-label]")?.textContent]).toEqual([
      "Your passkey may be ready. Log in with it.",
      "Continue with passkey",
    ]);
    answering("/admin");
    await press(host);
    expect(run).toHaveBeenCalledWith(...ran({ action: "login", base: "/admin", sent: {} }));
  });

  it("fails as usual when a login or an addition loses its answer, since neither spends a link", async () => {
    const host = island('action="add" form-token="F1"');
    ready(host, browser());
    losingTheAnswer();
    await press(host);
    await vi.waitFor(() => expect(host.dataset.state).toBe("failed"));
    expect(status(host)).toBe("Something went wrong. Try again in a minute.");
  });

  it("stays a join when it failed before its last request went out", async () => {
    const host = island('action="join"');
    ready(host, browser());
    run.mockRejectedValueOnce(new TypeError("Failed to fetch"));
    await press(host);
    await vi.waitFor(() => expect(host.dataset.state).toBe("failed"));
    expect(host.querySelector("[data-label]")?.textContent).toBe("Go");
  });
});

const HINT = "Still waiting. Try the device you set up.";

/** Has the next ceremony hang until its signal ends it, as a device that finds no passkey does, and keeps that
 * signal. */
function hanging(): { signal?: AbortSignal } {
  const kept: { signal?: AbortSignal } = {};
  run.mockImplementationOnce(
    (_asked, _device, signal) =>
      new Promise((_, refuse) => {
        kept.signal = signal;
        signal.addEventListener("abort", () => refuse(signal.reason));
      }),
  );
  return kept;
}

describe("a passkey island's clock", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("says it is still waiting after 15 seconds, and closes the prompt after a minute", async () => {
    const host = island('action="login"');
    ready(host, browser());
    const kept = hanging();
    button(host).click();
    await vi.advanceTimersByTimeAsync(14_999);
    expect(status(host)).toBe("Waiting for your device…");
    await vi.advanceTimersByTimeAsync(1);
    expect([status(host), host.dataset.state]).toEqual([HINT, "waiting"]);
    await vi.advanceTimersByTimeAsync(45_000);
    expect([kept.signal?.reason, host.dataset.state, button(host).disabled]).toEqual([
      "timed out",
      "failed",
      false,
    ]);
    expect(status(host)).toBe("Your device didn't answer. Try again.");
  });

  it("never ends a ceremony whose last request went out", async () => {
    const host = island('action="login"');
    ready(host, browser());
    let signal: AbortSignal | undefined;
    run.mockImplementationOnce((_asked, _device, given, finishing) => {
      signal = given;
      finishing();
      return new Promise(() => undefined);
    });
    button(host).click();
    await vi.advanceTimersByTimeAsync(61_000);
    expect([signal?.aborted, status(host), button(host).disabled]).toEqual([
      false,
      "Waiting for your device…",
      true,
    ]);
  });
});

describe("a passkey island's clock, across presses and states", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts the clock again for a later press", async () => {
    const host = island('action="login"');
    ready(host, browser());
    hanging();
    hanging();
    button(host).click();
    await vi.advanceTimersByTimeAsync(10_000);
    button(host).click();
    await vi.advanceTimersByTimeAsync(6_000);
    expect(status(host)).toBe("Waiting for your device…");
    await vi.advanceTimersByTimeAsync(9_000);
    expect(status(host)).toBe(HINT);
  });

  it("says a join timed out, and offers it again, since no passkey was kept", async () => {
    const host = island('action="join"');
    ready(host, browser());
    hanging();
    button(host).click();
    await vi.advanceTimersByTimeAsync(60_000);
    expect([host.dataset.state, button(host).disabled]).toEqual(["failed", false]);
    expect(status(host)).toBe("Your device didn't answer. Try again.");
    expect(run).toHaveBeenCalledTimes(1);
  });
});

describe("a passkey island's clock, once a ceremony ends", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("stops once a ceremony fails, so nothing is said over the failure", async () => {
    const host = island('action="login"');
    ready(host, browser());
    run.mockRejectedValueOnce(new Refused("this login didn't work. Try again"));
    button(host).click();
    await vi.advanceTimersByTimeAsync(61_000);
    expect([status(host), host.dataset.state]).toEqual(["This login didn't work. Try again.", "failed"]);
  });

  it("stops once a join is done, so nothing is said over it", async () => {
    const host = island('action="join"');
    ready(host, browser());
    run.mockResolvedValueOnce("/admin");
    button(host).click();
    await vi.advanceTimersByTimeAsync(61_000);
    expect(status(host)).toBe("Passkey created. Logging you in.");
    expect([host.dataset.state, run.mock.calls.length]).toEqual(["done", 1]);
  });

  it("stops once done, so nothing is said over it", async () => {
    const host = island('action="login"');
    ready(host, browser());
    run.mockResolvedValueOnce("/admin");
    button(host).click();
    await vi.advanceTimersByTimeAsync(61_000);
    expect([status(host), host.dataset.state]).toEqual(["Logged in.", "done"]);
  });
});

describe("a passkey island finishing", () => {
  it("lets no press cut off a ceremony's last request, and turns back on once it is answered", async () => {
    const host = island('action="join"');
    ready(host, browser());
    let answer: (error: unknown) => void = () => undefined;
    run.mockImplementationOnce((_asked, _device, _signal, finishing) => {
      finishing();
      return new Promise((_, refuse) => (answer = refuse));
    });
    button(host).click();
    await vi.waitFor(() => expect(button(host).disabled).toBe(true));
    button(host).click();
    expect(run).toHaveBeenCalledTimes(1);
    answer(new Refused("this invite has ended. Log in, or ask an admin for a new one"));
    await vi.waitFor(() => expect(host.dataset.state).toBe("failed"));
    expect(button(host).disabled).toBe(false);
  });
});

describe("a passkey island that cannot go on", () => {
  it("says why a ceremony did not finish, and stays", async () => {
    const host = island('action="login"');
    const page = browser();
    ready(host, page);
    run.mockRejectedValue(new Refused("this login didn't work. Try again"));
    await press(host);
    expect([status(host), host.dataset.state]).toEqual(["This login didn't work. Try again.", "failed"]);
    expect(page.location.assign).not.toHaveBeenCalled();
  });

  it("is left as it is with no button, label, status, action, or realm it knows", () => {
    for (const host of [
      // Each missing alone: the button, the label, and the status line.
      island('action="login"', '<span data-label>Go</span><p role="status"></p>'),
      island('action="login"', '<button type="button" disabled></button><p role="status"></p>'),
      island('action="login"', '<button type="button" disabled><span data-label>Go</span></button>'),
      island('action="delete"'),
      island(""),
      // A realm it cannot trust, or none.
      island('base="//evil.example" action="login"'),
      island('base="/admin/" action="login"'),
      withoutBase(island('action="login"')),
    ]) {
      ready(host, browser());
      expect(host.dataset.state).toBeUndefined();
      expect(host.querySelector("button")?.disabled ?? true).toBe(true);
    }
  });
});

describe("a passkey island in a browser without passkeys", () => {
  it("keeps a join's link where it cannot use passkeys, and says to open it where it can", () => {
    const page = { ...browser(), passkeys: undefined };
    const host = island('action="join"');
    ready(host, page);
    expect(page.history.replaceState).not.toHaveBeenCalled();
    expect(status(host)).toBe("This browser can't use passkeys. Open the invite in another.");
  });

  it("says so where the browser cannot use passkeys, and keeps the button off", () => {
    for (const lacking of [{ passkeys: undefined }, { credentials: undefined }]) {
      const host = island('action="login"');
      ready(host, { ...browser(), ...lacking });
      expect([button(host).disabled, status(host), host.dataset.state]).toEqual([
        true,
        "This browser can't use passkeys.",
        "failed",
      ]);
    }
  });

  it("is readied once, however often it is connected", async () => {
    const host = island('action="login"');
    const page = browser();
    ready(host, page);
    ready(host, page);
    run.mockResolvedValue("/admin");
    await press(host);
    expect(run).toHaveBeenCalledTimes(1);
  });
});

describe("the passkeys island", () => {
  function list(attributes: string): HTMLElement {
    const document = window.document.implementation.createHTMLDocument("test");
    document.body.innerHTML = `<tetsu-passkeys ${attributes}><ul></ul></tetsu-passkeys>`;
    return document.querySelector("tetsu-passkeys") as HTMLElement;
  }

  it("tells the admin's devices every passkey Tetsu holds", () => {
    const told = vi.fn().mockResolvedValue(undefined);
    signal(list('user="U1" credentials="A1 B2"'), browser({ signalAllAcceptedCredentials: told }));
    expect(told).toHaveBeenCalledWith({
      rpId: "admin.example.com",
      userId: "U1",
      allAcceptedCredentialIds: ["A1", "B2"],
    });
  });

  it("tells them none when the page lists none", () => {
    const told = vi.fn().mockResolvedValue(undefined);
    signal(list(""), browser({ signalAllAcceptedCredentials: told }));
    expect(told).toHaveBeenCalledWith({
      rpId: "admin.example.com",
      userId: "",
      allAcceptedCredentialIds: [],
    });
  });

  it("changes nothing when a device refuses to hear it", async () => {
    const told = vi.fn().mockRejectedValue(new DOMException("no", "SecurityError"));
    signal(list('user="U1"'), browser({ signalAllAcceptedCredentials: told }));
    await expect(told.mock.results[0]?.value).rejects.toThrow("no");
  });

  it("tells nothing in a browser that cannot", () => {
    for (const page of [browser(), { ...browser(), passkeys: undefined }]) {
      expect(() => signal(list('user="U1"'), page)).not.toThrow();
    }
  });
});

describe("define", () => {
  it("readies each island as it is connected to the page", () => {
    const told = vi.fn().mockResolvedValue(undefined);
    const registry = { define: vi.fn() } as unknown as CustomElementRegistry & {
      define: ReturnType<typeof vi.fn>;
    };
    define(registry, browser({ signalAllAcceptedCredentials: told }));
    const [[passkeyName, Passkey], [passkeysName, Passkeys]] = registry.define.mock.calls as [
      [string, CustomElementConstructor],
      [string, CustomElementConstructor],
    ];
    expect([passkeyName, passkeysName]).toEqual(["tetsu-passkey", "tetsu-passkeys"]);
    customElements.define("test-passkey", Passkey);
    customElements.define("test-passkeys", Passkeys);
    document.body.innerHTML =
      '<test-passkey base="/admin" action="login"><button disabled><span data-label>Go</span></button><p role="status"></p></test-passkey>' +
      '<test-passkeys user="U1" credentials="A1"></test-passkeys>';
    expect((document.querySelector("test-passkey") as HTMLElement).dataset.state).toBe("ready");
    expect(told).toHaveBeenCalledWith({
      rpId: "admin.example.com",
      userId: "U1",
      allAcceptedCredentialIds: ["A1"],
    });
    document.body.innerHTML = "";
  });
});

describe("said", () => {
  it.each([
    [new Refused("this request was refused"), "This request was refused."],
    [new DOMException("ended", "AbortError"), "The prompt closed early. Try again."],
    [new DOMException("pending", "InvalidStateError"), "Close the open passkey prompt first."],
    [new DOMException("closed", "NotAllowedError"), "No passkey was chosen. Try again."],
    [new DOMException("odd", "SecurityError"), "Something went wrong. Try again in a minute."],
    [new TypeError("not base64url"), "Something went wrong. Try again in a minute."],
    // Only the browser's own errors are read by name.
    [
      Object.assign(new Error("ended"), { name: "AbortError" }),
      "Something went wrong. Try again in a minute.",
    ],
  ])("says what %s means", (error, words) => {
    expect(said(error)).toBe(words);
  });
});
