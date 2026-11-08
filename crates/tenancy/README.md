# Tenancy

The businesses that use Tetsu, the addresses that reach them, and their themes and locations. Tier 2 until
[ADR-0020](../../docs/adr/0020-branch-coverage.md) is accepted ([#51](https://github.com/heavymedl-labs/tetsu/issues/51)),
which moves it to Tier 1.

## Hot paths

- **`resolve`.** Every request finds its tenant by address: one indexed lookup and one location query.
  Benchmarks come with [#37](https://github.com/heavymedl-labs/tetsu/issues/37).
