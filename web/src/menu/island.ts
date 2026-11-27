// The menu island (ADR-0031): <tetsu-menu> wraps a native popover menu, such as the settings shell's narrow-screen
// sidebar (web/templates/settings.html), which opens and closes without script. The island keeps focus where it can be
// seen (WCAG 2.4.11): opening the menu moves focus to its current page's link, or else its first control, and focus
// leaving it closes it, so focus never rests on what the menu covers. A browser without popovers shows the menu in
// place, and the island leaves it alone.
//
// It also closes the menu itself, so the menu slides out in every engine: WebKit, in Safari and every iOS browser,
// has no way to transition a popover out of the top layer, and hides one at once. The island makes the popover
// manual, then on its close button, Escape, a tap on the backdrop, or focus leaving, marks it `data-closing`, which
// its classes slide out, and hides it once that transition ends. Without script, the popover closes on its own.
//
// While the menu is open, the rest of the page is inert, so nothing behind its backdrop can be tapped or reached, as
// behind a modal; the page's stylesheet keeps the page from scrolling then (web/src/tetsu.css).

/** How long a close may take before the menu is hidden anyway: its 200ms transition, with room to spare. */
export const CLOSING_LIMIT = 400;

/** Readies a `<tetsu-menu>`: its popover takes focus as it opens, and slides out before it closes. */
export function ready(host: HTMLElement): void {
  const menu = host.querySelector<HTMLElement>("[popover]");
  if (!menu || !("hidePopover" in menu)) {
    return;
  }
  menu.setAttribute("popover", "manual");
  let open = false;
  let made: Element[] = [];
  /** Makes everything outside the menu inert while it is open, and only what this island made inert live again. */
  const isolate = (on: boolean) => {
    for (const element of made) {
      element.removeAttribute("inert");
    }
    made = [];
    const body = menu.ownerDocument.body;
    for (
      let inside: Element = menu;
      on && inside !== body && inside.parentElement;
      inside = inside.parentElement
    ) {
      for (const sibling of inside.parentElement.children) {
        if (sibling !== inside && !sibling.hasAttribute("inert")) {
          sibling.setAttribute("inert", "");
          made.push(sibling);
        }
      }
    }
  };
  const close = () => {
    if (!open || menu.hasAttribute("data-closing")) {
      return;
    }
    if (globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      menu.hidePopover();
      return;
    }
    menu.setAttribute("data-closing", "");
    const done = () => {
      clearTimeout(timer);
      menu.removeEventListener("transitionend", ended);
      menu.removeAttribute("data-closing");
      menu.hidePopover();
    };
    const ended = (event: Event) => {
      if (event.target === menu && (event as TransitionEvent).propertyName === "translate") {
        done();
      }
    };
    const timer = setTimeout(done, CLOSING_LIMIT);
    menu.addEventListener("transitionend", ended);
  };
  menu.addEventListener("toggle", (event) => {
    open = (event as ToggleEvent).newState === "open";
    isolate(open);
    if (open) {
      const current = menu.querySelector<HTMLElement>("[aria-current=page]");
      (current ?? menu.querySelector<HTMLElement>("a, button"))?.focus();
    }
  });
  menu.addEventListener("focusout", (event) => {
    const next = event.relatedTarget;
    if (next instanceof Node && !menu.contains(next)) {
      close();
    }
  });
  menu.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  });
  menu.addEventListener("click", (event) => {
    if ((event.target as Element).closest("[popovertargetaction=hide]")) {
      event.preventDefault();
      close();
    }
  });
  // A tap on the backdrop lands on the page behind it, outside the menu. A menu taken off the page closes nothing.
  host.ownerDocument.addEventListener("click", (event) => {
    if (open && menu.isConnected && !menu.contains(event.target as Node)) {
      event.preventDefault();
      close();
    }
  });
}

/** Registers the island. The module is deferred, so it is readied as soon as it runs, for the markup already
 * parsed. */
export function define(registry: CustomElementRegistry): void {
  registry.define(
    "tetsu-menu",
    class extends HTMLElement {
      connectedCallback(): void {
        ready(this);
      }
    },
  );
}
