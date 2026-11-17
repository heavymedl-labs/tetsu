import { describe, expect, it } from "vitest";
import { define, kept, ready } from "./island";

/** A `<tetsu-scheme>` as the page renders it, with the browser's cookie jar it writes to. */
function island() {
  const document = window.document.implementation.createHTMLDocument("test");
  document.body.innerHTML =
    '<tetsu-scheme><fieldset><input type="radio" name="scheme" value="system">' +
    '<input type="radio" name="scheme" value="light"><input type="radio" name="scheme" value="dark">' +
    '<input type="text" name="other" value="x"></fieldset></tetsu-scheme>';
  const host = document.querySelector("tetsu-scheme") as HTMLElement;
  const page = { cookie: "" };
  return { host, page };
}

/** Picks a radio, as a click does, and reports the change. */
function choose(host: HTMLElement, value: string) {
  const radio = host.querySelector(`input[value="${value}"]`) as HTMLInputElement;
  radio.checked = true;
  radio.dispatchEvent(new Event("change", { bubbles: true }));
}

describe("kept", () => {
  it("keeps light or dark for a year on the whole site, and removes the cookie for the system's", () => {
    expect(kept("dark")).toBe("theme=dark; Path=/; Max-Age=31536000; Secure; SameSite=Lax");
    expect(kept("light")).toBe("theme=light; Path=/; Max-Age=31536000; Secure; SameSite=Lax");
    for (const other of ["system", "", "Dark", "blue"]) {
      expect(kept(other), other).toBe("theme=; Path=/; Max-Age=0; Secure; SameSite=Lax");
    }
  });
});

describe("a scheme island", () => {
  it("keeps each choice in the theme cookie as it is made", () => {
    const { host, page } = island();
    ready(host, page);
    choose(host, "dark");
    expect(page.cookie).toBe(kept("dark"));
    choose(host, "system");
    expect(page.cookie).toBe(kept("system"));
  });

  it("keeps nothing for a change to anything but its radios", () => {
    const { host, page } = island();
    ready(host, page);
    host.querySelector('input[name="other"]')?.dispatchEvent(new Event("change", { bubbles: true }));
    host.dispatchEvent(new Event("change"));
    expect(page.cookie).toBe("");
  });

  it("registers <tetsu-scheme>, readying each one connected", () => {
    const page = { cookie: "" };
    define(window.customElements, page);
    const host = document.createElement("tetsu-scheme");
    host.innerHTML = '<input type="radio" name="scheme" value="light">';
    document.body.append(host);
    choose(host, "light");
    expect(page.cookie).toBe(kept("light"));
    host.remove();
  });
});
