import { afterEach, expect, it } from "vitest";

afterEach(() => {
  document.body.innerHTML = "";
});

it("registers the scheme island with the browser's own document", async () => {
  await import("./admin");
  expect(customElements.get("tetsu-scheme")).toBeDefined();
});
