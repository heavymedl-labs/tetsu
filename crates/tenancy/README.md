# Tenancy

The businesses that use Tetsu, the addresses that reach them, and their themes and locations. Tier 2 until it
meets the Tier 1 bar ([#63](https://github.com/heavymedl-labs/tetsu/issues/63)), since it decides which tenant
a request reaches.

## Hot paths

- **`resolve`.** Every request finds its tenant by address: one indexed lookup and one location query.
  Benchmarks come with [#37](https://github.com/heavymedl-labs/tetsu/issues/37).
