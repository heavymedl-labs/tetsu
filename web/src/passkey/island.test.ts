import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Refused } from "../shared/http";
import { ceremony } from "./ceremony";
import { type Browser, define, ready, said, signal } from "./island";

// The ceremony is ceremony.test.ts's; here it only answers, so each island's handling of it shows.
vi.mock("./ceremony", async (original) => ({
  ...(await original<typeof import("./ceremony")>()),
  ceremony: vi.fn(),
}));
const run = vi.mocked(ceremony);

const device = {} as CredentialsContainer;

/** A browser that can use passkeys, at an invite link's address. */
function browser(passkeys: Partial<typeof PublicKeyCredential> = {}): Browser {
  return {
    location: {
      hash: "#L1",
      pathname: "/ops/join",
      search: "?from=mail",
      hostname: "ops.example.com",
      assign: vi.fn(),
    },
    history: { replaceState: vi.fn() },
    credentials: device,
    passkeys: passkeys as typeof PublicKeyCredential,
  };
}

/** A `<tetsu-passkey>` as the server renders it, with `attributes` and `inside` as its content. */
function island(
  attributes: string,
  inside = '<button type="button" disabled><span data-label>Go</span></button><p role="status"></p>',
): HTMLElement {
  const document = window.document.implementation.createHTMLDocument("test");
  document.body.innerHTML = `<tetsu-passkey ${attributes}>${inside}</tetsu-passkey>`;
  return document.querySelector("tetsu-passkey") as HTMLElement;
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
  run.mockImplementationOnce(async (_action, _sent, _device, _signal, finishing) => {
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
    run.mockResolvedValue("/ops");
    await press(host);
    expect(run).toHaveBeenCalledWith("login", {}, device, expect.any(AbortSignal), expect.any(Function));
    expect(page.location.assign).toHaveBeenCalledWith("/ops");
    expect(page.history.replaceState).not.toHaveBeenCalled();
    expect([status(host), host.dataset.state]).toEqual(["Logged in.", "done"]);
  });
});

describe("a passkey island that joins", () => {
  it("takes an invite link from the address, and clears it from the bar at once", () => {
    const page = browser();
    ready(island('action="join"'), page);
    expect(page.history.replaceState).toHaveBeenCalledWith(null, "", "/ops/join?from=mail");
  });

  it("logs in with a new passkey at once, on the same page (ADR-0067)", async () => {
    const host = island('action="join"');
    const page = browser();
    ready(host, page);
    let signedIn: (next: string) => void = () => undefined;
    run
      .mockResolvedValueOnce("/ops/login")
      .mockReturnValueOnce(new Promise((resolve) => (signedIn = resolve)));
    button(host).click();
    await vi.waitFor(() => expect(status(host)).toBe("Passkey created. Confirm once more to log in."));
    expect(host.dataset.state).toBe("waiting");
    signedIn("/ops");
    await vi.waitFor(() => expect(host.dataset.state).toBe("done"));
    expect(run.mock.calls).toEqual([
      ["join", { link: "L1" }, device, expect.any(AbortSignal), expect.any(Function)],
      ["login", {}, device, expect.any(AbortSignal), expect.any(Function)],
    ]);
    expect([status(host), host.querySelector("[data-label]")?.textContent]).toEqual([
      "Logged in.",
      "Continue with passkey",
    ]);
    expect(page.location.assign).toHaveBeenCalledWith("/ops");
  });
});

describe("a passkey island whose chained login is refused", () => {
  it("leaves the button to log in when the browser refuses the chained prompt", async () => {
    const host = island('action="join"');
    const page = browser();
    ready(host, page);
    run.mockResolvedValueOnce("/ops/login").mockRejectedValueOnce(new DOMException("no", "NotAllowedError"));
    await press(host);
    await vi.waitFor(() => expect(host.dataset.state).toBe("ready"));
    expect(status(host)).toBe("Passkey created. Continue with passkey to log in.");
    expect(page.location.assign).not.toHaveBeenCalled();
    answering("/ops");
    await press(host);
    expect(run).toHaveBeenCalledWith("login", {}, device, expect.any(AbortSignal), expect.any(Function));
    expect(page.location.assign).toHaveBeenCalledWith("/ops");
  });
});

describe("a passkey island and what it sends", () => {
  it("sends a person whose session ended to log in, to return here after", async () => {
    run.mockRejectedValueOnce(new Refused("you are logged out; log in again", 401));
    const host = island('action="add" form-token="F1"');
    const page = browser();
    page.location.pathname = "/ops";
    ready(host, page);
    await press(host);
    expect(page.location.assign).toHaveBeenCalledWith("/ops/login?ended=1&next=/ops");
    expect([status(host), host.dataset.state]).toEqual(["You are logged out; log in again.", "failed"]);
  });

  it("sends the page a login returns to, which the server checks again, or nothing the page lacks", async () => {
    for (const [attributes, sent] of [
      ['action="login" next="/ops/passkeys"', { next: "/ops/passkeys" }],
      ['action="login"', {}],
    ] as const) {
      run.mockReset();
      run.mockResolvedValue("/ops/passkeys");
      const host = island(attributes);
      ready(host, browser());
      await press(host);
      expect(run).toHaveBeenCalledWith("login", sent, device, expect.any(AbortSignal), expect.any(Function));
    }
  });

  it("sends the operator's form token when adding a passkey, or none the page lacks", async () => {
    for (const [attributes, form] of [
      ['action="add" form-token="F1"', "F1"],
      ['action="add"', ""],
    ]) {
      run.mockReset();
      run.mockResolvedValue("/ops");
      const host = island(attributes as string);
      ready(host, browser());
      await press(host);
      expect(run).toHaveBeenCalledWith(
        "add",
        { form },
        device,
        expect.any(AbortSignal),
        expect.any(Function),
      );
      expect(status(host)).toBe("Passkey added.");
    }
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
    finish("/ops");
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
      .mockImplementationOnce((_action, _sent, _device, signal) => {
        signals.push(signal);
        return new Promise((_, refuse) =>
          signal.addEventListener("abort", () => refuse(new DOMException("ended", "AbortError"))),
        );
      })
      .mockImplementationOnce((_action, _sent, _device, signal) => {
        signals.push(signal);
        return new Promise((resolve) => (second = resolve));
      });
    button(host).click();
    button(host).click();
    await vi.waitFor(() => expect(signals).toHaveLength(2));
    await new Promise((settled) => setTimeout(settled));
    expect(signals.map((signal) => signal.aborted)).toEqual([true, false]);
    expect([status(host), host.dataset.state]).toEqual(["Waiting for your device…", "waiting"]);
    second("/ops");
    await vi.waitFor(() => expect(host.dataset.state).toBe("done"));
    expect(button(host).disabled).toBe(true);
    expect(page.location.assign).toHaveBeenCalledTimes(1);
  });
});

describe("a passkey island whose chained login is refused or ended", () => {
  it("keeps the chained login's refusal from the server, rather than offering to log in", async () => {
    const host = island('action="join"');
    ready(host, browser());
    run
      .mockResolvedValueOnce("/ops/login")
      .mockRejectedValueOnce(new Refused("this login could not be completed"));
    await press(host);
    await vi.waitFor(() => expect(host.dataset.state).toBe("failed"));
    expect(status(host)).toBe("This login could not be completed.");
  });

  it("says nothing of a chained login another press ended", async () => {
    const host = island('action="join"');
    ready(host, browser());
    let chained: AbortSignal | undefined;
    run.mockResolvedValueOnce("/ops/login").mockImplementationOnce((_action, _sent, _device, signal) => {
      chained = signal;
      return new Promise((_, refuse) =>
        signal.addEventListener("abort", () => refuse(new DOMException("ended", "AbortError"))),
      );
    });
    run.mockImplementationOnce(() => new Promise(() => undefined));
    button(host).click();
    await vi.waitFor(() => expect(chained).toBeDefined());
    button(host).click();
    await new Promise((settled) => setTimeout(settled));
    expect([status(host), host.dataset.state]).toEqual(["Waiting for your device…", "waiting"]);
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
      "Your passkey may have been created. Continue with passkey to log in.",
      "Continue with passkey",
    ]);
    answering("/ops");
    await press(host);
    expect(run).toHaveBeenCalledWith("login", {}, device, expect.any(AbortSignal), expect.any(Function));
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

const HINT =
  "Still waiting. If your device can't find or make a passkey for Tetsu, cancel and use the one you set up, or ask an operator for a new invite.";

/** Has the next ceremony hang until its signal ends it, as a device that finds no passkey does, and keeps that
 * signal. */
function hanging(): { signal?: AbortSignal } {
  const kept: { signal?: AbortSignal } = {};
  run.mockImplementationOnce(
    (_action, _sent, _device, signal) =>
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
    expect(status(host)).toBe(
      "Your device did not answer within a minute, so the prompt was closed. Try again, or use the device you set up.",
    );
  });

  it("never ends a ceremony whose last request went out", async () => {
    const host = island('action="login"');
    ready(host, browser());
    let signal: AbortSignal | undefined;
    run.mockImplementationOnce((_action, _sent, _device, given, finishing) => {
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

  it("leaves the button to log in when a chained login times out", async () => {
    const host = island('action="join"');
    ready(host, browser());
    // The join's last request stops its clock, as a real one does, so only the chained login's own runs.
    run.mockImplementationOnce(async (_action, _sent, _device, _signal, finishing) => {
      finishing();
      return "/ops/login";
    });
    hanging();
    button(host).click();
    await vi.advanceTimersByTimeAsync(0);
    expect(status(host)).toBe("Passkey created. Confirm once more to log in.");
    await vi.advanceTimersByTimeAsync(60_000);
    expect([status(host), host.dataset.state]).toEqual([
      "Passkey created. Continue with passkey to log in.",
      "ready",
    ]);
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
    run.mockRejectedValueOnce(new Refused("this login could not be completed"));
    button(host).click();
    await vi.advanceTimersByTimeAsync(61_000);
    expect([status(host), host.dataset.state]).toEqual(["This login could not be completed.", "failed"]);
  });

  it("stops once a chained login falls back to its button", async () => {
    const host = island('action="join"');
    ready(host, browser());
    run.mockResolvedValueOnce("/ops/login").mockRejectedValueOnce(new DOMException("no", "NotAllowedError"));
    button(host).click();
    await vi.advanceTimersByTimeAsync(61_000);
    expect(status(host)).toBe("Passkey created. Continue with passkey to log in.");
  });

  it("stops once done, so nothing is said over it", async () => {
    const host = island('action="login"');
    ready(host, browser());
    run.mockResolvedValueOnce("/ops");
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
    run.mockImplementationOnce((_action, _sent, _device, _signal, finishing) => {
      finishing();
      return new Promise((_, refuse) => (answer = refuse));
    });
    button(host).click();
    await vi.waitFor(() => expect(button(host).disabled).toBe(true));
    button(host).click();
    expect(run).toHaveBeenCalledTimes(1);
    answer(new Refused("this invite has ended; ask an operator for a new one"));
    await vi.waitFor(() => expect(host.dataset.state).toBe("failed"));
    expect(button(host).disabled).toBe(false);
  });
});

describe("a passkey island that cannot go on", () => {
  it("says why a ceremony did not finish, and stays", async () => {
    const host = island('action="login"');
    const page = browser();
    ready(host, page);
    run.mockRejectedValue(new Refused("this login could not be completed"));
    await press(host);
    expect([status(host), host.dataset.state]).toEqual(["This login could not be completed.", "failed"]);
    expect(page.location.assign).not.toHaveBeenCalled();
  });

  it("is left as it is with no button, label, status, or action it knows", () => {
    for (const host of [
      // Each missing alone: the button, the label, and the status line.
      island('action="login"', '<span data-label>Go</span><p role="status"></p>'),
      island('action="login"', '<button type="button" disabled></button><p role="status"></p>'),
      island('action="login"', '<button type="button" disabled><span data-label>Go</span></button>'),
      island('action="delete"'),
      island(""),
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
    expect(status(host)).toBe("This browser cannot use passkeys. Open this link in one that can.");
  });

  it("says so where the browser cannot use passkeys, and keeps the button off", () => {
    for (const lacking of [{ passkeys: undefined }, { credentials: undefined }]) {
      const host = island('action="login"');
      ready(host, { ...browser(), ...lacking });
      expect([button(host).disabled, status(host), host.dataset.state]).toEqual([
        true,
        "This browser cannot use passkeys.",
        "failed",
      ]);
    }
  });

  it("is readied once, however often it is connected", async () => {
    const host = island('action="login"');
    const page = browser();
    ready(host, page);
    ready(host, page);
    run.mockResolvedValue("/ops");
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

  it("tells the operator's devices every passkey Tetsu holds", () => {
    const told = vi.fn().mockResolvedValue(undefined);
    signal(list('user="U1" credentials="A1 B2"'), browser({ signalAllAcceptedCredentials: told }));
    expect(told).toHaveBeenCalledWith({
      rpId: "ops.example.com",
      userId: "U1",
      allAcceptedCredentialIds: ["A1", "B2"],
    });
  });

  it("tells them none when the page lists none", () => {
    const told = vi.fn().mockResolvedValue(undefined);
    signal(list(""), browser({ signalAllAcceptedCredentials: told }));
    expect(told).toHaveBeenCalledWith({ rpId: "ops.example.com", userId: "", allAcceptedCredentialIds: [] });
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
      '<test-passkey action="login"><button disabled><span data-label>Go</span></button><p role="status"></p></test-passkey>' +
      '<test-passkeys user="U1" credentials="A1"></test-passkeys>';
    expect((document.querySelector("test-passkey") as HTMLElement).dataset.state).toBe("ready");
    expect(told).toHaveBeenCalledWith({
      rpId: "ops.example.com",
      userId: "U1",
      allAcceptedCredentialIds: ["A1"],
    });
    document.body.innerHTML = "";
  });
});

describe("said", () => {
  it.each([
    [new Refused("this request was refused"), "This request was refused."],
    [new DOMException("ended", "AbortError"), "The passkey prompt closed before it finished. Try again."],
    [
      new DOMException("pending", "InvalidStateError"),
      "Finish or close the passkey prompt that is already open, then try again.",
    ],
    [
      new DOMException("closed", "NotAllowedError"),
      "The passkey prompt closed without a passkey. Try again when you are ready.",
    ],
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
