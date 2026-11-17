import { describe, expect, it } from "vitest";
import { loggedOut } from "./login";

describe("loggedOut", () => {
  it("is the login page, saying the session ended, and returning to the page after", () => {
    expect(loggedOut("/ops/passkeys")).toBe("/ops/login?ended=1&next=/ops/passkeys");
    expect(loggedOut("/ops/a/b")).toBe("/ops/login?ended=1&next=/ops/a/b");
  });

  it("leaves out /ops, where a login goes anyway, and a path it could not return to as written", () => {
    for (const path of ["/ops", "/ops/a&b", "/ops/a%2Fb", "/opsx", "/ops/", "/login", "/x/ops/a"]) {
      expect(loggedOut(path), path).toBe("/ops/login?ended=1");
    }
  });
});
