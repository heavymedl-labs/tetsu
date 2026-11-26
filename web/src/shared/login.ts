// A realm's base path, and where a page goes once its session has ended, which the passkey and session islands
// share.

/** A realm's base path as a page names it: none, for a realm at the root of its own host (ADR-0094), or a slash and
 * lowercase letters. A customer realm's `/<business>/account` (ADR-0084) needs more, which its slice adds. */
const BASE = /^(\/[a-z]+)?$/;

/** A path written plainly, in the alphabet of the server's return path check: what a login may return to, below its
 * realm's base path. The base itself is where a login goes anyway, so it needs no saying. */
const PLAIN = /^(\/[A-Za-z0-9._-]+)+$/;

/** The realm's base path a page gives an island, or none for anything else, so an island never calls an address a
 * page could bend. */
export function realmBase(value: string | null): string | undefined {
  return value?.match(BASE)?.[0];
}

/** The realm's login page, saying the session ended, and returning to `pathname` after when it is a page below `base`
 * written plainly, which the server checks again; any other path would arrive changed or goes to the realm's home
 * anyway, so it is left out. */
export function loggedOut(base: string, pathname: string): string {
  const login = `${base}/login?ended=1`;
  return pathname.startsWith(`${base}/`) && PLAIN.test(pathname) ? `${login}&next=${pathname}` : login;
}
