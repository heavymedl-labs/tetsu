# Tetsu

Tetsu (working name) is a multi-tenant vertical SaaS platform in Rust. The
**platform** supplies capabilities every vertical needs; each **vertical** plugs in
as a module and holds its own core domain. Boutique fitness is the first vertical.
Architecture: [ADR-0001](docs/adr/0001-modular-monolith-with-hexagonal-ports.md).
Market research: [Vertical SaaS Market Scan](https://claude.ai/artifact/RNRowRrBHCAfoFTMcKLxoJ).

## Vocabulary

- **Platform capability**: a module every vertical can use. Three groups:
  - Control plane: tenancy, identity, authorization, theming, audit, import
  - Shared domain: party, documents, scheduling, booking, attendance, catalog,
    entitlements, pricing, orders, inventory, subscriptions, ledger, staff,
    domain events, segments, workflows, analytics
  - Adapters: payments, notifications, media, integrations, public API, webhooks
- **Vertical**: a domain module (e.g. fitness) built on platform capabilities. Its
  rules (spot maps, late-cancel fees, intro offers) never go into the platform until
  a second vertical needs them.
- **Port**: a Rust trait a capability exposes or depends on. **Adapter**: code that
  implements or calls a port (Postgres, Stripe, HTTP).

## Layout

- `crates/<capability>` – one crate per capability, named `tetsu-<capability>`
- `crates/server` – the `tetsu` binary, the composition root; nothing depends on it
- `docs/adr/` – Architecture Decision Records

## Rules

- No crate reads another module's tables or internals; talk through ports and
  domain events only. A new cross-crate dependency needs a reason in the PR.
- Every operation on tenant data takes a `TenantContext` (`tetsu-tenancy`).
- Money is integer minor units plus currency, never floats. The ledger is
  append-only; corrections are reversals.
- No `unsafe` (forbidden workspace-wide). Cryptography only from vetted crates.
- Card data never touches Tetsu; the payment processor holds it.
- New dependencies must pass `cargo deny check`; add licenses to `deny.toml` only
  deliberately.
- Significant decisions get an ADR (see `docs/adr/README.md`) citing sources.

## Risk tiers

1. Auth, the money ledger, tenant isolation: highest bar, property and
   adversarial tests, two-step review.
2. Booking and scheduling: thorough unit and module tests.
3. UI: tests on behavior that matters.

## Commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check          # needs: cargo install cargo-deny
cargo run -p tetsu
```

## Workflow

Branch from `main`, keep PRs small, and make CI (fmt, clippy, tests, cargo-deny)
pass before review. Never push directly to `main`.
