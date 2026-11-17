import { afterEach, expect, it, vi } from "vitest";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = "";
});

const PARTS = "<dialog><span data-left></span><button data-stay></button><form></form></dialog>";

/** Loads the entry afresh, with `fetch` answering every read, and connects one island. */
async function loaded(): Promise<{ host: HTMLElement; fetch: ReturnType<typeof vi.fn> }> {
  const fetch = vi.fn().mockResolvedValue(new Response('{"idle":900,"absolute":28800}'));
  vi.stubGlobal("fetch", fetch);
  await import("./ops");
  document.body.innerHTML = `<tetsu-session form-token="F1">${PARTS}</tetsu-session>`;
  return { host: document.querySelector("tetsu-session") as HTMLElement, fetch };
}

it("registers the session island with the browser's own parts and a channel to the site's other tabs", async () => {
  const opened = vi.fn();
  vi.stubGlobal(
    "BroadcastChannel",
    class {
      constructor(name: string) {
        opened(name);
      }
      addEventListener() {}
      postMessage() {}
    },
  );
  const { host, fetch } = await loaded();
  expect([host.dataset.ready, opened.mock.calls]).toEqual(["", [["tetsu-session"]]]);
  expect(fetch).toHaveBeenCalledWith("/ops/session", { signal: null });
});
