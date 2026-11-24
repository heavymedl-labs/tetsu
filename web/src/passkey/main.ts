// The passkey pages' script, in every realm: it registers the passkey islands (./island.ts) with the browser's own
// parts.

import { define } from "./island";

define(customElements, {
  location,
  history,
  credentials: navigator.credentials,
  passkeys: globalThis.PublicKeyCredential,
});
