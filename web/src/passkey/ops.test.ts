import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ceremony, Refused } from "./ceremony";
import { type Page, said, start } from "./ops";

// The ceremony is ceremony.test.ts's; here it only answers, so each page's handling of it shows.
vi.mock("./ceremony", async (original) => ({
  ...(await original<typeof import("./ceremony")>()),
  ceremony: vi.fn(),
}));
const run = vi.mocked(ceremony);

const device = {} as CredentialsContainer;

/** A page with `body` as its markup, at an enrollment link's address, with a browser that can use passkeys. */
function page(body: string, passkeys: Partial<typeof PublicKeyCredential> = {}): Page {
  const document = window.document.implementation.createHTMLDocument("test");
  document.body.innerHTML = body;
  return {
    document,
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

const parts = (action: string, extra = "") =>
  `<button data-passkey="${action}"${extra} disabled></button><p data-passkey-status></p>`;

function button(shown: Page): HTMLButtonElement {
  return shown.document.querySelector("button") as HTMLButtonElement;
}

function status(shown: Page): string | null | undefined {
  return shown.document.querySelector("[data-passkey-status]")?.textContent;
}

/** Presses the page's button and waits until the page has said or done what it does with the ceremony's end. */
async function press(shown: Page): Promise<void> {
  button(shown).click();
  await vi.waitFor(() => expect(run).toHaveBeenCalled());
  await new Promise((settled) => setTimeout(settled));
}

beforeEach(() => {
  run.mockReset();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("start", () => {
  it("signs in when pressed, and goes where the ceremony says", async () => {
    const shown = page(parts("sign-in"));
    start(shown);
    expect(button(shown).disabled).toBe(false);
    run.mockResolvedValue("/ops");
    await press(shown);
    expect(run).toHaveBeenCalledWith("sign-in", {}, device);
    expect(shown.location.assign).toHaveBeenCalledWith("/ops");
    expect(shown.history.replaceState).not.toHaveBeenCalled();
    expect(status(shown)).toBe("Waiting for your device…");
  });

  it("takes an enrollment link from the address, and clears it from the bar at once", async () => {
    const shown = page(parts("enroll"));
    start(shown);
    expect(shown.history.replaceState).toHaveBeenCalledWith(null, "", "/ops/enroll?from=mail");
    run.mockResolvedValue("/ops/sign-in");
    await press(shown);
    expect(run).toHaveBeenCalledWith("enroll", { link: "L1" }, device);
  });

  it("sends the operator's form token when adding a passkey, or none the page lacks", async () => {
    for (const [extra, form] of [
      [' data-form="F1"', "F1"],
      ["", ""],
    ]) {
      run.mockReset();
      run.mockResolvedValue("/ops");
      const shown = page(parts("add", extra));
      start(shown);
      await press(shown);
      expect(run).toHaveBeenCalledWith("add", { form }, device);
    }
  });
});

describe("start, when it cannot go on", () => {
  it("says why a ceremony did not finish, and stays", async () => {
    const shown = page(parts("sign-in"));
    start(shown);
    run.mockRejectedValue(new Refused("this sign-in could not be completed"));
    await press(shown);
    expect(status(shown)).toBe("This sign-in could not be completed.");
    expect(shown.location.assign).not.toHaveBeenCalled();
  });

  it("leaves a page with no button or no status as it is", () => {
    for (const body of ["<p data-passkey-status></p>", '<button data-passkey="sign-in" disabled></button>']) {
      const shown = page(body);
      start(shown);
      expect(shown.document.querySelector("button")?.disabled ?? true).toBe(true);
      expect(status(shown) ?? "").toBe("");
    }
  });

  it("says so where the browser cannot use passkeys, and keeps the button off", () => {
    for (const lacking of [{ passkeys: undefined }, { credentials: undefined }]) {
      const shown = { ...page(parts("sign-in")), ...lacking };
      start(shown);
      expect(button(shown).disabled).toBe(true);
      expect(status(shown)).toBe("This browser cannot use passkeys.");
    }
  });
});

describe("the passkeys Tetsu holds", () => {
  const account = '<ul data-account data-user="U1" data-credentials="A1 B2"></ul>';

  it("are told to the operator's devices", () => {
    const signal = vi.fn().mockResolvedValue(undefined);
    start(page(account, { signalAllAcceptedCredentials: signal }));
    expect(signal).toHaveBeenCalledWith({
      rpId: "ops.example.com",
      userId: "U1",
      allAcceptedCredentialIds: ["A1", "B2"],
    });
  });

  it("are told as none when the page lists none", () => {
    const signal = vi.fn().mockResolvedValue(undefined);
    start(page("<ul data-account></ul>", { signalAllAcceptedCredentials: signal }));
    expect(signal).toHaveBeenCalledWith({
      rpId: "ops.example.com",
      userId: "",
      allAcceptedCredentialIds: [],
    });
  });

  it("change nothing when a device refuses to hear them", async () => {
    const signal = vi.fn().mockRejectedValue(new DOMException("no", "SecurityError"));
    start(page(account, { signalAllAcceptedCredentials: signal }));
    await expect(signal.mock.results[0]?.value).rejects.toThrow("no");
  });

  it("are not told by a browser that cannot, or from a page with no account", () => {
    const signal = vi.fn();
    start(page("<p></p>", { signalAllAcceptedCredentials: signal }));
    start(page(account));
    start({ ...page(account), passkeys: undefined });
    expect(signal).not.toHaveBeenCalled();
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
