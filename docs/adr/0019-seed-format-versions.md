# 0019. Seed format versions

- Status: Proposed
- Date: 2026-11-07
- Supersedes the versioning rule in [ADR-0018](0018-demo-businesses-as-data.md)

## Context

[ADR-0018](0018-demo-businesses-as-data.md) made seed files a versioned TOML format that refuses unknown
fields. It also promised two things that cannot both hold. Adding an optional field was to keep the format
version, and an older binary was to report a newer file as newer, not as a typo. An older binary does not know
the new field, so it refuses that field as unknown, and the file's version gives no warning. A review of the
`tetsu seed` code found this in November 2026.

## Decision

**Every change to the format raises `format`, including a new optional field.**

- The reader checks `format` before anything else, as ADR-0018 says. A binary that meets a higher version
  reports it as newer and names the version it reads.
- `format` stays a top-level integer in every version, since older binaries find it there.
- Unknown fields stay an error, so typos are still caught.
- Tetsu keeps reading the previous version until the demos repository moves, as ADR-0018 says.

The rest of ADR-0018 stands.

## Alternatives considered

- **Keep optional fields at the same version.** No bump for small changes, but an older binary then reports a
  file that uses the new field as having an unknown field, which reads as a typo.
- **Ignore unknown fields.** Older binaries would read newer files, but a misspelled field would be skipped
  without a word. ADR-0018 rejected that.

## Consequences

Error messages stay honest about why a file fails. The cost is a version bump, and a reader for the previous
version, for each field added. While the demos repository is the only producer of seed files, that cost is
small.

## Sources

- [ADR-0018](0018-demo-businesses-as-data.md) and the reader in `crates/tetsu/src/seed.rs`.
- Serde documentation, "Container attributes": `deny_unknown_fields`.
