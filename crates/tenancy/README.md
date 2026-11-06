# Tenancy

The businesses that use Tetsu, the addresses that reach them, and their themes and locations. Tier 2 until
[#8](https://github.com/heavymedl-labs/tetsu/issues/8) decides isolation.

## Hot paths

- **`resolve`.** Every request finds its tenant by address: one indexed lookup and one location query.
  Benchmarks come with [#37](https://github.com/heavymedl-labs/tetsu/issues/37).
