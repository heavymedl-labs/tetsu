# 0021. Module public surface

- Status: Accepted
- Date: 2026-11-08

## Context

The invariant in [docs/invariants.md](../invariants.md) and the crate layout in
[ADR-0016](0016-crate-layout-and-coverage.md) say a module's public surface is its use cases, driven ports,
domain events, and `Router`. Tenancy, the first module, cannot meet that wording
([#52](https://github.com/heavymedl-labs/tetsu/issues/52)):

- Its use cases take and return domain types such as `NewTenant` and `Tenant`, so those types must be public.
- The binary registers it through a type that implements the kernel's `Module`.
- The binary, as composition root, builds its Postgres adapter, so the adapter's constructor must be public.

Because the port and the adapter constructor are public, any crate holding a pool can call
`TenantStore::insert` and skip the use cases. The maintainer chose to amend the invariant rather than narrow
the code.

## Decision

- **A module's public surface is** its use cases, its driven ports, its domain events, its `Router`, the domain
  types those name, the type that registers it with the kernel, and the constructors of its driven adapters.
- **An adapter constructor returns a handle that offers only the module's use cases,** never the port. The
  composition root builds the handle and passes it on, so no crate outside the module holds a value it could
  call a port's methods on. Use cases become methods on the handle, and a module whose use cases need two
  adapters builds its handle from both. Ports stay public so that tests and other adapters can implement them.
- **Other modules still talk only through use cases and domain events,** as the invariant says.
  [#25](https://github.com/heavymedl-labs/tetsu/issues/25) keeps its dependency check, and needs no method-call
  check for this rule.
- **On acceptance,** the invariant's wording changes to the first rule, ADR-0016's line for `lib.rs` is marked
  superseded by this ADR, and Tenancy's `postgres_store` returns the handle.

## Alternatives considered

- **Return the port and forbid calling it outside the module.** No change to the use cases, but nothing can
  enforce it: the composition root must name the port to pass the store to a use case, and calling the port's
  methods needs that same import and nothing more. Only a call-graph lint could tell the two apart, and #25
  plans none.
- **Narrow the code to the current wording** by moving each adapter into its own crate that only the binary
  depends on. That keeps constructors off the module's surface, but doubles the crates and splits a module
  across two, against ADR-0016's one crate per module.
- **Hide constructors behind a Cargo feature only the binary turns on.** Fewer crates, but features unify across
  the workspace, so any crate's dependency could turn it on.

## Consequences

- The invariant matches the code, so a reviewer can apply it as written.
- Skipping a use case is closed by the types, not by a check or a review.
- Tenancy's use cases move onto the handle, and its tests and the seed loader change to match.
- [ARCHITECTURE.md](../../ARCHITECTURE.md#module)'s module view gains the public surface on acceptance.

**Weakest evidence.** One module so far, with one adapter and no domain events or `Router` yet.

**Revisit** if a module's handle grows awkward, such as one built from many adapters, or if a second kind of
driven adapter needs wiring the composition root cannot do through a constructor.

## Sources

- [AWS Prescriptive Guidance: Hexagonal architecture](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/hexagonal-architecture.html):
  adapters are wired at the edge, and the core exposes ports.
- [Evans: Domain-Driven Design Reference](https://www.domainlanguage.com/ddd/reference/): bounded contexts
  expose a published language, here the domain types their use cases name.
