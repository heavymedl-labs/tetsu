import { afterEach, describe, expect, it, vi } from "vitest";
import { ceremony, Refused } from "./ceremony";
import fixture from "./fixtures/options.json";

/** The server's answers, in order, as `fetch` gives them. */
function serve(...answers: [number, unknown][]) {
  const fetch = vi.fn();
  for (const [status, body] of answers) {
    fetch.mockResolvedValueOnce(new Response(JSON.stringify(body), { status }));
  }
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

/** What `fetch` was asked to send, as each call's path and JSON body. */
function sent(fetch: ReturnType<typeof vi.fn>) {
  return fetch.mock.calls.map(([path, init]) => {
    expect(init).toMatchObject({ method: "POST", headers: { "Content-Type": "application/json" } });
    return [path, JSON.parse(init.body)];
  });
}

const buffer = (...values: number[]) => new Uint8Array(values).buffer;

const signed = {
  id: "AQ",
  rawId: buffer(1),
  type: "public-key",
  response: {
    clientDataJSON: buffer(2),
    authenticatorData: buffer(3),
    signature: buffer(4),
    userHandle: null,
  },
  getClientExtensionResults: () => ({}),
};

const created = {
  ...signed,
  response: { clientDataJSON: buffer(2), attestationObject: buffer(5), getTransports: () => ["internal"] },
};

function device(credential: unknown) {
  return {
    get: vi.fn().mockResolvedValue(credential),
    create: vi.fn().mockResolvedValue(credential),
  } as unknown as CredentialsContainer & { get: ReturnType<typeof vi.fn>; create: ReturnType<typeof vi.fn> };
}

/** A signal no one has ended. */
const live = () => new AbortController().signal;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("ceremony", () => {
  it("signs in: begins, asks the device for a passkey, finishes, and says where to go", async () => {
    const fetch = serve([200, { ceremony: "c1", options: fixture.request }], [200, { next: "/ops" }]);
    const phone = device(signed);
    expect(await ceremony("sign-in", {}, phone, live(), vi.fn())).toBe("/ops");
    const asked = phone.get.mock.calls[0]?.[0] as CredentialRequestOptions;
    expect(asked.signal).toBeInstanceOf(AbortSignal);
    expect(asked.publicKey?.rpId).toBe("ops.example.com");
    expect(phone.create).not.toHaveBeenCalled();
    expect(sent(fetch)).toEqual([
      ["/ops/sign-in/begin", {}],
      [
        "/ops/sign-in/finish",
        {
          ceremony: "c1",
          response: {
            id: "AQ",
            rawId: "AQ",
            type: "public-key",
            response: { clientDataJSON: "Ag", authenticatorData: "Aw", signature: "BA", userHandle: null },
            clientExtensionResults: {},
          },
        },
      ],
    ]);
  });
});

describe("ceremony, creating a passkey", () => {
  it("enrolls and adds by creating a passkey, sending what the page gave with both halves", async () => {
    for (const [action, endpoint, extra] of [
      ["enroll", "/ops/enroll", { link: "L" }],
      ["add", "/ops/passkeys", { form: "F" }],
    ] as const) {
      const fetch = serve(
        [200, { ceremony: "c2", options: fixture.creation }],
        [200, { next: "/ops/sign-in" }],
      );
      const laptop = device(created);
      expect(await ceremony(action, extra, laptop, live(), vi.fn())).toBe("/ops/sign-in");
      expect(laptop.get).not.toHaveBeenCalled();
      const asked = laptop.create.mock.calls[0]?.[0] as CredentialCreationOptions;
      expect(asked.publicKey?.rp.name).toBe("Tetsu");
      const calls = sent(fetch);
      expect(calls[0]).toEqual([`${endpoint}/begin`, extra]);
      expect(calls[1]?.[0]).toBe(`${endpoint}/finish`);
      expect(calls[1]?.[1]).toMatchObject({
        ...extra,
        ceremony: "c2",
        response: { response: { transports: ["internal"] } },
      });
    }
  });
});

describe("a ceremony that does not finish", () => {
  it("throws the server's refusal, in its words", async () => {
    serve([400, { error: "this sign-in could not be completed" }]);
    const refused = await ceremony("sign-in", {}, device(signed), live(), vi.fn()).catch(
      (error: unknown) => error,
    );
    expect(refused).toBeInstanceOf(Refused);
    expect((refused as Refused).message).toBe("this sign-in could not be completed");
  });

  it("throws a plain refusal when the server gives no words, and when finishing is refused", async () => {
    serve([403, { error: 1 }]);
    await expect(ceremony("sign-in", {}, device(signed), live(), vi.fn())).rejects.toThrow(
      new Refused("this request was refused"),
    );
    serve([200, { ceremony: "c", options: fixture.request }], [400, { error: "no" }]);
    await expect(ceremony("sign-in", {}, device(signed), live(), vi.fn())).rejects.toThrow(new Refused("no"));
  });
});

describe("a ceremony refused by the device or a proxy", () => {
  it("treats no passkey from the device as the browser's refusal, finishing nothing", async () => {
    const fetch = serve([200, { ceremony: "c", options: fixture.request }]);
    const finishing = vi.fn();
    const refused = await ceremony("sign-in", {}, device(null), live(), finishing).catch(
      (error: unknown) => error,
    );
    const { name, message } = refused as DOMException;
    expect([refused instanceof DOMException, name, message]).toEqual([
      true,
      "NotAllowedError",
      "the device gave no passkey",
    ]);
    expect([fetch.mock.calls.length, finishing.mock.calls.length]).toEqual([1, 0]);
  });

  it("takes a refusal whose body is not JSON, as a proxy may send, for the server's", async () => {
    const fetch = vi.fn().mockResolvedValue(new Response("<h1>Bad gateway</h1>", { status: 502 }));
    vi.stubGlobal("fetch", fetch);
    await expect(ceremony("sign-in", {}, device(signed), live(), vi.fn())).rejects.toThrow(
      new Refused("this request was refused"),
    );
  });

  it("says it is finishing just before its last request, once", async () => {
    const fetch = serve([200, { ceremony: "c", options: fixture.request }], [200, { next: "/ops" }]);
    const finishing = vi.fn(() => expect(fetch).toHaveBeenCalledTimes(1));
    await ceremony("sign-in", {}, device(signed), live(), finishing);
    expect([finishing.mock.calls.length, fetch.mock.calls.length]).toEqual([1, 2]);
  });

  it("throws when the server says nowhere to go next, as no refusal, since the server kept what it was sent", async () => {
    serve([200, { ceremony: "c", options: fixture.request }], [200, {}]);
    const thrown = await ceremony("sign-in", {}, device(signed), live(), vi.fn()).catch(
      (error: unknown) => error,
    );
    expect([thrown instanceof Refused, (thrown as Error).message]).toEqual([
      false,
      "the server said nowhere to go next",
    ]);
  });
});

describe("a ceremony a later press ended", () => {
  it("sends nothing more, and says nothing of finishing, once a later press ended it", async () => {
    const fetch = serve([200, { ceremony: "c", options: fixture.request }]);
    const ending = new AbortController();
    const phone = device(signed);
    phone.get.mockImplementationOnce(async () => {
      ending.abort();
      return signed;
    });
    const finishing = vi.fn();
    await expect(ceremony("sign-in", {}, phone, ending.signal, finishing)).rejects.toThrow();
    expect([finishing.mock.calls.length, fetch.mock.calls.length]).toEqual([0, 1]);
  });
});

describe("a ceremony's signal", () => {
  it("reaches both requests and the device's prompt, so ending it ends every part", async () => {
    const fetch = serve([200, { ceremony: "c", options: fixture.request }], [200, { next: "/ops" }]);
    const phone = device(signed);
    const signal = live();
    await ceremony("sign-in", {}, phone, signal, vi.fn());
    const signals = fetch.mock.calls.map(([, init]) => init.signal);
    expect(signals).toEqual([signal, signal]);
    expect((phone.get.mock.calls[0] as [CredentialRequestOptions])[0].signal).toBe(signal);
  });
});
