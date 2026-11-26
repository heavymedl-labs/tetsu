import { describe, expect, it } from "vitest";
import { loggedOut, realmBase } from "./login";

describe("realmBase", () => {
  it("is a realm's base path as a page names it", () => {
    expect([realmBase("/admin"), realmBase("/app")]).toEqual(["/admin", "/app"]);
  });

  it("is none, an empty base, for a realm at the root of its own host", () => {
    expect(realmBase("")).toBe("");
  });

  it("is none for anything else, so an island never calls an address a page could bend", () => {
    for (const value of [
      null,
      "/",
      "admin",
      "/admin/",
      "/Admin",
      "//evil.example",
      "/a/b",
      "https://evil.example",
      "/admin?x=1",
      "/ad min",
      "/admin\n",
    ]) {
      expect(realmBase(value), String(value)).toBeUndefined();
    }
  });
});

describe("loggedOut", () => {
  it("at a host's root, is its login page, returning to any page written plainly", () => {
    expect(loggedOut("", "/settings/profile")).toBe("/login?ended=1&next=/settings/profile");
    expect(loggedOut("", "/mailbox")).toBe("/login?ended=1&next=/mailbox");
    for (const path of ["/", "", "/a&b", "/a%2Fb", "/settings/", "//evil.example"]) {
      expect(loggedOut("", path), path).toBe("/login?ended=1");
    }
  });

  it("is the realm's login page, saying the session ended, and returning to the page after", () => {
    expect(loggedOut("/admin", "/admin/passkeys")).toBe("/admin/login?ended=1&next=/admin/passkeys");
    expect(loggedOut("/admin", "/admin/a/b")).toBe("/admin/login?ended=1&next=/admin/a/b");
    expect(loggedOut("/app", "/app/settings/profile")).toBe("/app/login?ended=1&next=/app/settings/profile");
  });

  it("leaves out the realm's own address, where a login goes anyway, and a path it could not return to as written", () => {
    for (const path of [
      "/admin",
      "/admin/a&b",
      "/admin/a%2Fb",
      "/adminx",
      "/admin/",
      "/login",
      "/x/admin/a",
      "/app/settings",
      "/admin/a&b/c",
      "/admin/a/b&c",
    ]) {
      expect(loggedOut("/admin", path), path).toBe("/admin/login?ended=1");
    }
    // Plain, but in another realm.
    expect(loggedOut("/app", "/xyz/page")).toBe("/app/login?ended=1");
  });
});
