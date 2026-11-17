// Where a page goes once its session has ended, which the passkey and session islands share.

/** A path the login may return to as written: what the server's return path check accepts, in its alphabet. */
const PLAIN = /^\/ops(\/[A-Za-z0-9._-]+)*$/;

/** The login page, saying the session ended, and returning to `pathname` after when it is written plainly, which
 * the server checks again; any other path would arrive changed, so it is left out. */
export function loggedOut(pathname: string): string {
  return PLAIN.test(pathname) ? `/ops/login?ended=1&next=${pathname}` : "/ops/login?ended=1";
}
