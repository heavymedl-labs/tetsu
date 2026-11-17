import { beforeEach, describe, expect, it, vi } from "vitest";
import { ceremony, Refused } from "./ceremony";
import { type Browser, define, ready, said, signal } from "./island";

// The ceremony is ceremony.test.ts's; here it only answers, so each island's handling of it shows.
vi.mock("./ceremony", async (original) => ({
  ...(await original<typeof import("./ceremony")>()),
  ceremony: vi.fn(),
}));
const run = vi.mocked(ceremony);

const device = {} as CredentialsContainer;

/** A browser that can use passkeys, at an enrollment link's address. */
function browser(passkeys: Partial<typeof PublicKeyCredential> = {}): Browser {
  return {
    location: {
      hash: "#L1",
      pathname: "/ops/enroll",
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
  inside = '<button type="button" disabled></button><p role="status"></p>',
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

beforeEach(() => {
  run.mockReset();
});

describe("a passkey island", () => {
  it("signs in when pressed, and goes where the ceremony says", async () => {
    const host = island('action="sign-in"');
    const page = browser();
    ready(host, page);
    expect([button(host).disabled, host.dataset.state]).toEqual([false, "ready"]);
    run.mockResolvedValue("/ops");
    await press(host);
    expect(run).toHaveBeenCalledWith("sign-in", {}, device);
    expect(page.location.assign).toHaveBeenCalledWith("/ops");
    expect(page.history.replaceState).not.toHaveBeenCalled();
    expect([status(host), host.dataset.state]).toEqual(["Waiting for your device…", "waiting"]);
  });

  it("takes an enrollment link from the address, and clears it from the bar at once", async () => {
    const host = island('action="enroll"');
    const page = browser();
    ready(host, page);
    expect(page.history.replaceState).toHaveBeenCalledWith(null, "", "/ops/enroll?from=mail");
    run.mockResolvedValue("/ops/sign-in");
    await press(host);
    expect(run).toHaveBeenCalledWith("enroll", { link: "L1" }, device);
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
      expect(run).toHaveBeenCalledWith("add", { form }, device);
    }
  });
});

describe("a passkey island that cannot go on", () => {
  it("says why a ceremony did not finish, and stays", async () => {
    const host = island('action="sign-in"');
    const page = browser();
    ready(host, page);
    run.mockRejectedValue(new Refused("this sign-in could not be completed"));
    await press(host);
    expect([status(host), host.dataset.state]).toEqual(["This sign-in could not be completed.", "failed"]);
    expect(page.location.assign).not.toHaveBeenCalled();
  });

  it("is left as it is with no button, no status, or no action it knows", () => {
    for (const host of [
      island('action="sign-in"', '<p role="status"></p>'),
      island('action="sign-in"', '<button type="button" disabled></button>'),
      island('action="delete"'),
      island(""),
    ]) {
      ready(host, browser());
      expect(host.dataset.state).toBeUndefined();
      expect(host.querySelector("button")?.disabled ?? true).toBe(true);
    }
  });

  it("says so where the browser cannot use passkeys, and keeps the button off", () => {
    for (const lacking of [{ passkeys: undefined }, { credentials: undefined }]) {
      const host = island('action="sign-in"');
      ready(host, { ...browser(), ...lacking });
      expect([button(host).disabled, status(host), host.dataset.state]).toEqual([
        true,
        "This browser cannot use passkeys.",
        "failed",
      ]);
    }
  });

  it("is readied once, however often it is connected", async () => {
    const host = island('action="sign-in"');
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
      '<test-passkey action="sign-in"><button disabled></button><p role="status"></p></test-passkey>' +
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
    [new DOMException("ended", "AbortError"), ""],
    [
      new DOMException("closed", "NotAllowedError"),
      "The passkey prompt closed without a passkey. Try again when you are ready.",
    ],
    [new DOMException("odd", "InvalidStateError"), "Something went wrong. Try again in a minute."],
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
