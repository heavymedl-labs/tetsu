// A logged-in page's session script, in every realm: it registers the session island (./island.ts) with the
// browser's own parts.

import { channel, define } from "./island";

define(customElements, {
  location,
  page: document,
  window,
  channel,
});
