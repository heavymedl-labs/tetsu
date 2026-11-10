# Tenancy

The businesses that use Tetsu, the addresses that reach them, and their themes and locations. Tier 2 until it
meets the Tier 1 bar ([#63](https://github.com/heavymedl-labs/tetsu/issues/63)), since it decides which tenant
a request reaches.

## Hot paths

- **`resolve`.** Every request finds its tenant by address: one indexed lookup and one location query.
  Benchmarks come with [#37](https://github.com/heavymedl-labs/tetsu/issues/37).
- **The public page.** `router` serves `GET /<slug>`: one `resolve`, then an Askama render. A lookup that
  takes over 5 seconds, or fails, gets an unavailable page without the detail.
  It sets no fallback: the site sets the one axum allows. Names Tetsu's own pages need, such as `assets` and
  `login`, are reserved, so no slug can be one. `reserved_addresses_in_use` finds stored addresses a later
  release reserved, which a site route would hide, and `tetsu serve` refuses to start while any is held
  ([#90](https://github.com/heavymedl-labs/tetsu/issues/90)).
