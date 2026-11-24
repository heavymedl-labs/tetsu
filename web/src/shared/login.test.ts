import { describe, expect, it } from "vitest";
import { loggedOut } from "./login";

describe("loggedOut", () => {
  it("is the login page, saying the session ended, and returning to the page after", () => {
    expect(loggedOut("/admin/passkeys")).toBe("/admin/login?ended=1&next=/admin/passkeys");
    expect(loggedOut("/admin/a/b")).toBe("/admin/login?ended=1&next=/admin/a/b");
  });

  it("leaves out /admin, where a login goes anyway, and a path it could not return to as written", () => {
    for (const path of [
      "/admin",
      "/admin/a&b",
      "/admin/a%2Fb",
      "/adminx",
      "/admin/",
      "/login",
      "/x/admin/a",
    ]) {
      expect(loggedOut(path), path).toBe("/admin/login?ended=1");
    }
  });
});
