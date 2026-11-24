// The script of each page with the color scheme control: it registers the scheme island (./island.ts) with the
// browser's own document.

import { define } from "./island";

define(customElements, document);
