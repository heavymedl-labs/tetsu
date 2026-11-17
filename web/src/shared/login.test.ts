import { describe, expect, it } from "vitest";
import { loggedOut } from "./login";

describe("loggedOut", () => {
  it("is the login page, saying the session ended, and returning to the page after", () => {
    expect(loggedOut("/ops/passkeys")).toBe("/ops/login?ended=1&next=/ops/passkeys");
  });
});
