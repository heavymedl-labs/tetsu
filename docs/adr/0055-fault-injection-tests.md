# 0055. Fault injection tests

- Status: Proposed
- Date: 2026-11-13

## Context

Tests already cover a pool out of connections, a commit Postgres refuses, and a stale cached statement, by provoking
each through Postgres itself. Nothing tests the network between the app and Postgres failing: a connection reset
mid-transaction, a database that answers slowly, or one that vanishes and returns, as Neon's does when it wakes from
zero. Those are how a hosted database fails in practice, and Tier 1 code
([ADR-0005](0005-testing-and-performance.md)) must fail safely through them
([#130](https://github.com/heavymedl-labs/tetsu/issues/130)).

Each check run uses one shared test Postgres, so a test cannot stop it without breaking the others running beside
it. Load runs are a separate decision ([#37](https://github.com/heavymedl-labs/tetsu/issues/37)).

Criteria, before looking at options: faults are injected between the app and Postgres without touching the shared
database; tests run in `scripts/check.sh` and CI, in parallel; no fault code ships in the binary; and the fewest new
tools. Evidence is in the [research](../research/browser-and-system-testing.md#system-testing).

## Decision

- **Use [Toxiproxy](https://github.com/Shopify/toxiproxy),** Shopify's TCP proxy for testing (MIT), as a container
  beside the test Postgres: started by hand once locally, as the test Postgres is, and as a service container in CI,
  pinned to an exact version by digest.
- **Each test makes its own proxy** to the test Postgres on a free port, through Toxiproxy's HTTP API from a small
  test helper, and removes it when done, so parallel tests never share a fault. The Rust client, last released in
  2021, is not used.
- **The faults tested first,** each asserting a clean error, no partial write, and a pool that recovers:
  - a connection reset in the middle of a transaction;
  - latency past the pool's wait timeout, which must give a temporary error, not a hung request;
  - the proxy cut and restored, as when Neon suspends and wakes;
  - a connection that stalls without closing.
- **They run on every check,** like the other Postgres tests, since each takes seconds.

## Alternatives considered

- **Stop or restart a Postgres container from the test.** Real, but needs a database per test, slow to start, or
  breaks the shared one.
- **In-process fail points,** such as `fail-parallel` 0.6.0. Precise, but they put fault hooks in production code
  behind a feature, and they cannot fail a real socket. Worth revisiting for faults inside Tetsu, not between it and
  Postgres.
- **Testcontainers.** Would start Toxiproxy per test, but Rust has no Toxiproxy module, and the project starts its
  test services once rather than per test.
- **Chaos Mesh or LitmusChaos.** Need Kubernetes, which Tetsu does not run.

## Consequences

- Toxiproxy 2.13.0 was released on 2026-10-14, MIT, by Shopify; read 2026-11-13.
- Contributors start one more container, which `scripts/check.sh` names when it is missing, as it does for the test
  Postgres. CI gains a service container.
- Tests learn how Tetsu fails before a real outage shows it. The login libraries' failure tests
  ([#109](https://github.com/heavymedl-labs/tetsu/issues/109)) can use the same helper.
- Replacing Toxiproxy means rewriting one helper; the tests name faults, not the tool.
- Revisit if faults inside Tetsu need testing too, or the project moves to per-test containers.

## Sources

- Shopify, [Toxiproxy README](https://github.com/Shopify/toxiproxy): toxics and HTTP API, read 2026-11-13.
- Neon, [plans](https://neon.com/docs/introduction/plans) and scale to zero, read 2026-11-13.
