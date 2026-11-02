# Tetsu

A multi-tenant vertical SaaS platform written in Rust (working name). Shared
platform capabilities – tenancy, identity, scheduling, booking, billing and ledger –
with verticals plugged in on top. Boutique fitness is the first vertical.

- Architecture: [ADR-0001](docs/adr/0001-modular-monolith-with-hexagonal-ports.md)
- Contributor and agent guide: [CLAUDE.md](CLAUDE.md)

## Quick start

```sh
cargo test --workspace
cargo run -p tetsu
```
