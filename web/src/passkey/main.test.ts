import { afterEach, expect, it, vi } from "vitest";

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

// jsdom has no WebAuthn, so the entry is given the browser's passkey parts before it runs.
it("registers the passkey islands with the browser's own parts", async () => {
  const signalled = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("PublicKeyCredential", { signalAllAcceptedCredentials: signalled });
  vi.stubGlobal("navigator", { credentials: {} });
  await import("./main");
  document.body.innerHTML =
    '<tetsu-passkey base="/admin" action="login"><button disabled><span data-label>Go</span></button><p role="status"></p></tetsu-passkey>' +
    '<tetsu-passkeys user="U1" credentials="A1"></tetsu-passkeys>';
  const island = document.querySelector("tetsu-passkey") as HTMLElement;
  expect([island.dataset.state, island.querySelector("button")?.disabled]).toEqual(["ready", false]);
  expect(signalled).toHaveBeenCalledWith({
    rpId: location.hostname,
    userId: "U1",
    allAcceptedCredentialIds: ["A1"],
  });
});
