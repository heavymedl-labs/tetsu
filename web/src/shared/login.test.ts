import { describe, expect, it } from "vitest";
import { loggedOut } from "./login";

describe("loggedOut", () => {
  it("is the login page, saying the session ended, and returning to the page after", () => {
    expect(loggedOut("/ops/passkeys")).toBe("/ops/login?ended=1&next=/ops/passkeys");
    expect(loggedOut("/ops")).toBe("/ops/login?ended=1&next=/ops");
  });

  it("leaves out a path the login could not return to as written", () => {
    for (const path of ["/ops/a&b", "/ops/a%2Fb", "/opsx", "/ops/", "/login", "/x/ops/a"]) {
      expect(loggedOut(path), path).toBe("/ops/login?ended=1");
    }
  });
});
