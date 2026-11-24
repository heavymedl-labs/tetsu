import { afterEach, describe, expect, it, vi } from "vitest";
import { define, ready } from "./island";

afterEach(() => {
  document.body.innerHTML = "";
});

/** A `<tetsu-menu>` as the settings shell renders it: a popover with a close button and links, one of them the
 * current page's, beside the control that opens it and a control after it. jsdom has no popover methods, so the
 * popover gets a stand-in `hidePopover`, which a browser without popovers lacks. */
function island(current = true, popovers = true) {
  document.body.innerHTML =
    '<button id="opener">Menu</button><tetsu-menu><div popover><button id="close">Close</button>' +
    '<a href="/profile" id="profile">Profile</a>' +
    `<a href="/security" id="security"${current ? ' aria-current="page"' : ""}>Security</a></div></tetsu-menu>` +
    '<button id="after">After</button>';
  const host = document.querySelector("tetsu-menu") as HTMLElement;
  const menu = host.querySelector("[popover]") as HTMLElement;
  const hide = vi.fn();
  if (popovers) {
    Object.assign(menu, { hidePopover: hide });
  }
  const byId = (id: string) => document.getElementById(id) as HTMLElement;
  return { host, menu, hide, byId };
}

/** Readies the island, then shows its popover: jsdom never shows one, and focuses nothing inside one it hides. */
function readied(host: HTMLElement, menu: HTMLElement) {
  ready(host);
  menu.removeAttribute("popover");
}

/** The browser's report that the popover opened or closed. */
function toggled(menu: HTMLElement, newState: "open" | "closed") {
  menu.dispatchEvent(Object.assign(new Event("toggle"), { newState }));
}

/** Focus leaving `from` for `to`, as Tab or a click moves it. */
function leave(from: HTMLElement, to: HTMLElement | null) {
  from.dispatchEvent(new FocusEvent("focusout", { bubbles: true, relatedTarget: to }));
}

describe("a menu island, opening", () => {
  it("moves focus to the current page's link when the menu opens", () => {
    const { host, menu, byId } = island();
    readied(host, menu);
    byId("opener").focus();
    toggled(menu, "open");
    expect(document.activeElement).toBe(byId("security"));
  });

  it("moves focus to the menu's first control when no link is the current page's", () => {
    const { host, menu, byId } = island(false);
    readied(host, menu);
    toggled(menu, "open");
    expect(document.activeElement).toBe(byId("close"));
  });

  it("leaves focus alone when the menu closes", () => {
    const { host, menu, byId } = island();
    readied(host, menu);
    byId("opener").focus();
    toggled(menu, "closed");
    expect(document.activeElement).toBe(byId("opener"));
  });

  it("opens a menu with nothing to focus without failing", () => {
    const { host, menu, byId } = island();
    menu.replaceChildren();
    readied(host, menu);
    byId("opener").focus();
    expect(() => toggled(menu, "open")).not.toThrow();
    expect(document.activeElement).toBe(byId("opener"));
  });
});

describe("a menu island, closing", () => {
  it("closes the open menu when focus leaves it, so focus never rests on what the menu covers", () => {
    const { host, menu, hide, byId } = island();
    readied(host, menu);
    toggled(menu, "open");
    leave(byId("security"), byId("after"));
    expect(hide).toHaveBeenCalledOnce();
  });

  it("stays open while focus moves within it, or leaves the page", () => {
    const { host, menu, hide, byId } = island();
    readied(host, menu);
    toggled(menu, "open");
    leave(byId("profile"), byId("security"));
    leave(byId("security"), null);
    expect(hide).not.toHaveBeenCalled();
  });

  it("closes nothing before the menu first opens", () => {
    const { host, menu, hide, byId } = island();
    readied(host, menu);
    leave(byId("security"), byId("after"));
    expect(hide).not.toHaveBeenCalled();
  });

  it("closes nothing once the menu is closed", () => {
    const { host, menu, hide, byId } = island();
    readied(host, menu);
    toggled(menu, "open");
    toggled(menu, "closed");
    leave(byId("security"), byId("after"));
    expect(hide).not.toHaveBeenCalled();
  });
});

describe("a menu island, set up", () => {
  it("leaves a browser without popovers alone, which shows the menu in place", () => {
    const { host, menu, byId } = island(true, false);
    readied(host, menu);
    byId("opener").focus();
    toggled(menu, "open");
    expect(document.activeElement).toBe(byId("opener"));
  });

  it("leaves a host without a popover alone", () => {
    document.body.innerHTML = "<tetsu-menu><a href='/x'>x</a></tetsu-menu>";
    expect(() => ready(document.querySelector("tetsu-menu") as HTMLElement)).not.toThrow();
  });

  it("registers <tetsu-menu>, readying each one connected", () => {
    define(window.customElements);
    const host = document.createElement("tetsu-menu");
    host.innerHTML = '<div popover><a href="/x" aria-current="page">x</a></div>';
    const menu = host.querySelector("[popover]") as HTMLElement;
    Object.assign(menu, { hidePopover: vi.fn() });
    document.body.append(host);
    menu.removeAttribute("popover");
    toggled(menu, "open");
    expect(document.activeElement).toBe(host.querySelector("a"));
    host.remove();
  });
});
