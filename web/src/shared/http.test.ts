import { afterEach, describe, expect, it, vi } from "vitest";
import { call, Refused } from "./http";

afterEach(() => {
  vi.unstubAllGlobals();
});

/** Stubs `fetch` with one answer. */
function answering(status: number, body: string) {
  const fetch = vi.fn().mockResolvedValue(new Response(body, { status }));
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

describe("call", () => {
  it("posts its body as JSON and reads the JSON answer", async () => {
    const fetch = answering(200, '{"next":"/ops"}');
    const signal = new AbortController().signal;
    expect(await call("/ops/login/begin", signal, { a: 1 })).toEqual({ next: "/ops" });
    expect(fetch).toHaveBeenCalledWith("/ops/login/begin", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: '{"a":1}',
      signal,
    });
  });

  it("gets, sending nothing, when there is no body", async () => {
    const fetch = answering(200, '{"idle":900}');
    const signal = new AbortController().signal;
    expect(await call("/ops/session", signal)).toEqual({ idle: 900 });
    expect(fetch).toHaveBeenCalledWith("/ops/session", { signal });
    await call("/ops/session");
    expect(fetch).toHaveBeenLastCalledWith("/ops/session", { signal: null });
  });

  it("throws the server's refusal with its words and status", async () => {
    answering(401, '{"error":"you are logged out; log in again"}');
    const refused = await call("/ops/session").catch((error: unknown) => error);
    expect(refused).toBeInstanceOf(Refused);
    expect([(refused as Refused).message, (refused as Refused).status]).toEqual([
      "you are logged out; log in again",
      401,
    ]);
  });

  it("takes a refusal without words, or not in JSON, as a plain one, keeping its status", async () => {
    for (const [status, body] of [
      [403, '{"error":1}'],
      [502, "<h1>Bad gateway</h1>"],
    ] as const) {
      answering(status, body);
      const refused = (await call("/ops/session").catch((error: unknown) => error)) as Refused;
      expect([refused.message, refused.status]).toEqual(["this request was refused", status]);
    }
  });
});

describe("Refused", () => {
  it("is a plain refusal, status 400, unless told otherwise", () => {
    expect(new Refused("no").status).toBe(400);
  });
});
