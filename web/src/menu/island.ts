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
// behind a modal, save live regions, which must still be heard; the page's stylesheet keeps the page from scrolling
// then (web/src/tetsu.css). Escape closes it wherever focus is, closing gives focus back to what opened it (WCAG
// 2.4.3), and the screen widening to show the sidebar hides it, so the page is never left inert.

/** How long a close may take before the menu is hidden anyway: its 200ms transition, with room to spare. */
export const CLOSING_LIMIT = 400;

/** The width from which the settings shell shows its sidebar, Tailwind's `md`, where the menu is hidden. */
const WIDE = "(min-width: 48rem)";

/** Makes `element` inert, into `made`, unless it is inert already, or is or holds a live region, whose announcements
 * must still be heard: then only its parts without one. */
function quiet(element: Element, made: Element[]): void {
  if (element.hasAttribute("inert") || element.matches("[aria-live]")) {
    return;
  }
  if (element.querySelector("[aria-live]")) {
    for (const part of element.children) {
      quiet(part, made);
    }
    return;
  }
  element.setAttribute("inert", "");
  made.push(element);
}

/** Makes everything outside `menu` inert, up to the body, and returns what it made inert. */
function isolate(menu: HTMLElement): Element[] {
  const made: Element[] = [];
  const body = menu.ownerDocument.body;
  for (let inside: Element = menu; inside !== body && inside.parentElement; inside = inside.parentElement) {
    for (const sibling of inside.parentElement.children) {
      if (sibling !== inside) {
        quiet(sibling, made);
      }
    }
  }
  return made;
}

/** Slides `menu` out, by marking it `data-closing`, and hides it once that slide ends, after `CLOSING_LIMIT` at the
 * latest, or at once where motion is reduced. */
function slideOut(menu: HTMLElement): void {
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
}

/** Readies a `<tetsu-menu>`: its popover takes focus as it opens, keeps the page behind it inert, slides out before
 * it closes, and gives focus back to what opened it. */
export function ready(host: HTMLElement): void {
  const menu = host.querySelector<HTMLElement>("[popover]");
  if (!menu || !("hidePopover" in menu)) {
    return;
  }
  menu.setAttribute("popover", "manual");
  const page = host.ownerDocument;
  let open = false;
  let made: Element[] = [];
  let opener: HTMLElement | null = null;
  const close = (event?: Event) => {
    event?.preventDefault();
    if (open && !menu.hasAttribute("data-closing")) {
      slideOut(menu);
    }
  };
  menu.addEventListener("toggle", (event) => {
    open = (event as ToggleEvent).newState === "open";
    for (const element of made) {
      element.removeAttribute("inert");
    }
    made = [];
    if (open) {
      opener =
        page.activeElement instanceof HTMLElement && !menu.contains(page.activeElement)
          ? page.activeElement
          : null;
      made = isolate(menu);
      const current = menu.querySelector<HTMLElement>("[aria-current=page]");
      (current ?? menu.querySelector<HTMLElement>("a, button"))?.focus();
    } else if (page.activeElement === page.body || menu.contains(page.activeElement)) {
      opener?.focus();
    }
  });
  menu.addEventListener("focusout", (event) => {
    const next = event.relatedTarget;
    if (next instanceof Node && !menu.contains(next)) {
      close();
    }
  });
  menu.addEventListener("click", (event) => {
    if ((event.target as Element).closest("[popovertargetaction=hide]")) {
      close(event);
    }
  });
  watch(menu, () => open && menu.isConnected, close);
}

/** Closes `menu`, while it is `shown`, from outside it: Escape wherever focus is, as a native popover's does, even on
 * the page's body after a tap on the menu's own background, and a tap on the backdrop, which lands outside the menu.
 * Once the screen is wide enough for the sidebar, the menu hides at once, so the page is never left inert behind it. */
function watch(menu: HTMLElement, shown: () => boolean, close: (event?: Event) => void): void {
  const page = menu.ownerDocument;
  page.addEventListener("keydown", (event) => shown() && event.key === "Escape" && close(event));
  page.addEventListener("click", (event) => {
    if (shown() && !menu.contains(event.target as Node)) {
      close(event);
    }
  });
  globalThis.matchMedia?.(WIDE).addEventListener?.("change", (event) => {
    if (shown() && (event as MediaQueryListEvent).matches) {
      menu.hidePopover();
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
