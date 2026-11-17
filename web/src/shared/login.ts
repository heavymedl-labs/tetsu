// Where a page goes once its session has ended, which the passkey and session islands share.

/** The login page, saying the session ended, and returning to `pathname` after, which the server checks. */
export function loggedOut(pathname: string): string {
  return `/ops/login?ended=1&next=${pathname}`;
}
