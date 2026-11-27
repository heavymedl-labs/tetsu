import { afterEach, describe, expect, it, vi } from "vitest";
import { CLOSING_LIMIT, define, ready } from "./island";

afterEach(() => {
  document.body.innerHTML = "";
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

/** A `<tetsu-menu>` as the settings shell renders it: a popover with a close button and links, one of them the
 * current page's, beside the control that opens it and a control after it. jsdom has no popover methods, so the
 * popover gets a stand-in `hidePopover`, which a browser without popovers lacks. */
function island(current = true, popovers = true) {
  document.body.innerHTML =
    '<button id="opener">Menu</button><tetsu-menu><div popover><button id="close" popovertargetaction="hide">Close</button>' +
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

/** The menu's slide out ending, or another transition, from the menu or one of its parts. */
function ended(target: HTMLElement, propertyName = "translate") {
  target.dispatchEvent(Object.assign(new Event("transitionend", { bubbles: true }), { propertyName }));
}

/** An open menu, readied, with timers the test moves. */
function opened() {
  vi.useFakeTimers();
  const parts = island();
  readied(parts.host, parts.menu);
  toggled(parts.menu, "open");
  return parts;
}

describe("a menu island, closing", () => {
  it("makes the popover manual, so it decides when the menu closes", () => {
    const { host, menu } = island();
    ready(host);
    expect(menu.getAttribute("popover")).toBe("manual");
  });

  it("slides the menu out when focus leaves it, and hides it once the slide ends", () => {
    const { menu, hide, byId } = opened();
    leave(byId("security"), byId("after"));
    expect([menu.getAttribute("data-closing"), hide.mock.calls.length]).toEqual(["", 0]);
    ended(menu);
    expect([menu.hasAttribute("data-closing"), hide.mock.calls.length]).toEqual([false, 1]);
    vi.advanceTimersByTime(CLOSING_LIMIT);
    expect(hide).toHaveBeenCalledOnce();
  });

  it("waits for its own slide, not a part's or another property's", () => {
    const { menu, hide, byId } = opened();
    leave(byId("security"), byId("after"));
    ended(byId("profile"));
    ended(menu, "opacity");
    expect(hide).not.toHaveBeenCalled();
    vi.advanceTimersByTime(CLOSING_LIMIT - 1);
    expect(hide).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect([menu.hasAttribute("data-closing"), hide.mock.calls.length]).toEqual([false, 1]);
  });

  it("hides the menu anyway when no slide ends, as when motion is off in its classes", () => {
    const { hide, byId } = opened();
    leave(byId("security"), byId("after"));
    vi.advanceTimersByTime(CLOSING_LIMIT);
    expect(hide).toHaveBeenCalledOnce();
  });

  it("closes once, however often it is asked while closing", () => {
    const { menu, hide, byId } = opened();
    leave(byId("security"), byId("after"));
    leave(byId("profile"), byId("after"));
    ended(menu);
    ended(menu);
    vi.advanceTimersByTime(CLOSING_LIMIT);
    expect(hide).toHaveBeenCalledOnce();
  });

  it("closes at once where motion is reduced", () => {
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: query === "(prefers-reduced-motion: reduce)",
    }));
    const { menu, hide, byId } = opened();
    leave(byId("security"), byId("after"));
    expect([menu.hasAttribute("data-closing"), hide.mock.calls.length]).toEqual([false, 1]);
  });

  it("slides where motion is allowed", () => {
    vi.stubGlobal("matchMedia", () => ({ matches: false }));
    const { menu, hide, byId } = opened();
    leave(byId("security"), byId("after"));
    expect([menu.hasAttribute("data-closing"), hide.mock.calls.length]).toEqual([true, 0]);
  });

  it("closes on Escape, which a manual popover leaves to it", () => {
    const { menu, byId } = opened();
    const pressed = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    byId("profile").dispatchEvent(pressed);
    expect([pressed.defaultPrevented, menu.hasAttribute("data-closing")]).toEqual([true, true]);
    const other = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    menu.removeAttribute("data-closing");
    byId("profile").dispatchEvent(other);
    expect(other.defaultPrevented).toBe(false);
  });

  it("closes from its close button, in place of the browser hiding it at once", () => {
    const { menu, byId } = opened();
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    byId("close").dispatchEvent(click);
    expect([click.defaultPrevented, menu.hasAttribute("data-closing")]).toEqual([true, true]);
  });

  it("closes from a tap on the backdrop, which lands outside the menu, in place of what it would have done", () => {
    const { menu, byId } = opened();
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    byId("after").dispatchEvent(click);
    expect([click.defaultPrevented, menu.hasAttribute("data-closing")]).toEqual([true, true]);
  });

  it("leaves a tap alone once its open menu is taken off the page", () => {
    const { menu } = opened();
    menu.remove();
    document.body.innerHTML = '<button id="later">Later</button>';
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    (document.getElementById("later") as HTMLElement).dispatchEvent(click);
    expect([click.defaultPrevented, menu.hasAttribute("data-closing")]).toEqual([false, false]);
  });

  it("leaves a tap outside alone while the menu is closed", () => {
    const { menu, byId } = opened();
    toggled(menu, "closed");
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    byId("after").dispatchEvent(click);
    expect([click.defaultPrevented, menu.hasAttribute("data-closing")]).toEqual([false, false]);
  });

  it("leaves a link's tap alone", () => {
    const { menu, byId } = opened();
    const click = new MouseEvent("click", { bubbles: true, cancelable: true, clientX: 5, clientY: 5 });
    byId("profile").dispatchEvent(click);
    expect([click.defaultPrevented, menu.hasAttribute("data-closing")]).toEqual([false, false]);
  });

  it("stays open while focus moves within it, or leaves the page", () => {
    const { menu, hide, byId } = opened();
    leave(byId("profile"), byId("security"));
    leave(byId("security"), null);
    vi.advanceTimersByTime(CLOSING_LIMIT);
    expect([menu.hasAttribute("data-closing"), hide.mock.calls.length]).toEqual([false, 0]);
  });

  it("closes nothing before the menu first opens", () => {
    vi.useFakeTimers();
    const { host, menu, hide, byId } = island();
    readied(host, menu);
    leave(byId("security"), byId("after"));
    vi.advanceTimersByTime(CLOSING_LIMIT);
    expect(hide).not.toHaveBeenCalled();
  });

  it("closes nothing once the menu is closed", () => {
    const { menu, hide, byId } = opened();
    toggled(menu, "closed");
    leave(byId("security"), byId("after"));
    vi.advanceTimersByTime(CLOSING_LIMIT);
    expect(hide).not.toHaveBeenCalled();
  });
});

describe("a menu island, modal", () => {
  it("makes everything outside the open menu inert, up to the body, and live again once it closes", () => {
    const { menu, byId, host } = opened();
    const inert = () =>
      [byId("opener"), byId("after"), host, menu, document.head].map((e) => e.hasAttribute("inert"));
    expect(inert()).toEqual([true, true, false, false, false]);
    expect(byId("after").getAttribute("inert")).toBe("");
    toggled(menu, "closed");
    expect(inert()).toEqual([false, false, false, false, false]);
  });

  it("leaves inert what was inert before it opened", () => {
    vi.useFakeTimers();
    const { host, menu, byId } = island();
    byId("after").setAttribute("inert", "");
    readied(host, menu);
    toggled(menu, "open");
    toggled(menu, "closed");
    expect([byId("after").hasAttribute("inert"), byId("opener").hasAttribute("inert")]).toEqual([
      true,
      false,
    ]);
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
