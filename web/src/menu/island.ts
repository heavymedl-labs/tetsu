// The menu island (ADR-0031): <tetsu-menu> wraps a native popover menu, such as the settings shell's narrow-screen
// sidebar (web/templates/settings.html), which opens and closes without script. The island keeps focus where it can be
// seen (WCAG 2.4.11): opening the menu moves focus to its current page's link, or else its first control, and focus
// leaving it closes it, so focus never rests on what the menu covers. A browser without popovers shows the menu in
// place, and the island leaves it alone.

/** Readies a `<tetsu-menu>`: its popover takes focus as it opens and closes as focus leaves it. */
export function ready(host: HTMLElement): void {
  const menu = host.querySelector<HTMLElement>("[popover]");
  if (!menu || !("hidePopover" in menu)) {
    return;
  }
  let open = false;
  menu.addEventListener("toggle", (event) => {
    open = (event as ToggleEvent).newState === "open";
    if (open) {
      const current = menu.querySelector<HTMLElement>("[aria-current=page]");
      (current ?? menu.querySelector<HTMLElement>("a, button"))?.focus();
    }
  });
  menu.addEventListener("focusout", (event) => {
    const next = event.relatedTarget;
    if (open && next instanceof Node && !menu.contains(next)) {
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
