import { afterEach, expect, it } from "vitest";

afterEach(() => {
  document.body.innerHTML = "";
});

it("registers the menu island with the browser's own registry", async () => {
  await import("./ops");
  expect(customElements.get("tetsu-menu")).toBeDefined();
});
