// Where a page goes once its session has ended, which the passkey and session islands share.

/** A page under `/admin` the login may return to as written: what the server's return path check accepts, in its
 * alphabet. `/admin` itself is where a login goes anyway, so it needs no saying. */
const PLAIN = /^\/admin(\/[A-Za-z0-9._-]+)+$/;

/** The login page, saying the session ended, and returning to `pathname` after when it is a page below `/admin`
 * written plainly, which the server checks again; any other path would arrive changed or goes to `/admin` anyway, so
 * it is left out. */
export function loggedOut(pathname: string): string {
  return PLAIN.test(pathname) ? `/admin/login?ended=1&next=${pathname}` : "/admin/login?ended=1";
}
