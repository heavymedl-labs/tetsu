// The color scheme island (ADR-0031, ADR-0070): <tetsu-scheme> wraps the page's System, Light, and Dark radios, which
// switch the page without script, and keeps the visitor's choice in the theme cookie, which the server reads so the
// next page arrives in it.

/** The part of the browser the island writes to, passed to `define` so tests can stand in for it. */
export type Page = Pick<Document, "cookie">;

/** A year, in seconds. */
const YEAR = 31_536_000;

/** The `Set-Cookie`-style text that keeps `choice`: light or dark for a year, on the whole site, and any other, the
 * system's, by removing the cookie, so the page follows the system again. It holds nothing secret, so script may read
 * it. */
export function kept(choice: string): string {
  const attributes = "Path=/; Max-Age";
  return choice === "light" || choice === "dark"
    ? `theme=${choice}; ${attributes}=${YEAR}; Secure; SameSite=Lax`
    : `theme=; ${attributes}=0; Secure; SameSite=Lax`;
}

/** Readies a `<tetsu-scheme>`: each change to one of its radios is kept. */
export function ready(host: HTMLElement, page: Page): void {
  host.addEventListener("change", (event) => {
    const target = event.target;
    if (target instanceof HTMLInputElement && target.name === "scheme") {
      page.cookie = kept(target.value);
    }
  });
}

/** Registers the island. The module is deferred, so it is readied as soon as it runs, for the markup already
 * parsed. */
export function define(registry: CustomElementRegistry, page: Page): void {
  registry.define(
    "tetsu-scheme",
    class extends HTMLElement {
      connectedCallback(): void {
        ready(this, page);
      }
    },
  );
}
