// Builds the assets of Tetsu's pages (ADR-0034). The tetsu crate embeds the output, which is not in git (ADR-0036).
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

const here = import.meta.dirname;

export default defineConfig({
  root: here,
  base: "/",
  publicDir: false,
  plugins: [tailwindcss()],
  build: {
    outDir: resolve(here, "../crates/tetsu/assets/build"),
    // Nothing else writes there, so each build empties it and no old file is left behind.
    emptyOutDir: true,
    manifest: true,
    // Every asset stays a file: the policy blocks the data: URIs Vite otherwise makes of small ones.
    assetsInlineLimit: 0,
    rolldownOptions: {
      // The stylesheet, the logged-in pages' scripts, and the brand files a page links to, which src/manifest.rs
      // names.
      input: {
        tetsu: resolve(here, "src/tetsu.css"),
        passkey: resolve(here, "src/passkey/main.ts"),
        session: resolve(here, "src/session/main.ts"),
        scheme: resolve(here, "src/scheme/main.ts"),
        menu: resolve(here, "src/menu/main.ts"),
        icon: resolve(here, "../docs/brand/tetsu-avatar.svg"),
        "touch-icon": resolve(here, "../docs/brand/tetsu-avatar.png"),
      },
    },
  },
});
