# 0018. Demo businesses as data

- Status: Accepted
- Date: 2026-11-07
- Versioning rule superseded by [ADR-0019](0019-seed-format-versions.md)
- Naming rule narrowed by [ADR-0037](0037-fictional-customers-on-tetsus-page.md)

## Context

The maintainer wants several demo sites soon, each testing a different use case, with no business data in
Tetsu's repository ([#39](https://github.com/heavymedl-labs/tetsu/issues/39)). A business using Tetsu is data,
not code, and the binary must never name one ([#19](https://github.com/heavymedl-labs/tetsu/issues/19)). The
public API ([#12](https://github.com/heavymedl-labs/tetsu/issues/12)) does not exist yet, so nothing outside
Tetsu can create a tenant through it.

[ADR-0012](0012-first-hosting-and-deploys.md) runs one environment on free plans with fictional tenants only.
Stripe moved from a test mode inside each account to separate sandboxes, which argues for keeping demo and real
data in separate environments.

## Decision

- **Demo businesses are data files in a private `heavymedl-labs/tetsu-demos` repository,** one tenant per file.
  Anvil Row and Juniper Lane are the first, following their [reference pages](../reference-businesses/README.md).
  Tetsu's own tests use neutral sample files and never a demo business.
- **`tetsu seed` applies them now, and the public API later.** The demos repository's CI runs the published
  `tetsu` image. Seeding checks every file before writing anything, writes nothing if one is invalid, and skips
  tenants that already exist. It creates tenants only through public use cases such as Tenancy's
  `create_tenant`, never with SQL, so a file cannot hold anything the domain would reject. Once #12 exists, the
  demos repository moves to the API and drops its database secret.
- **Tetsu owns a versioned TOML format,** documented beside the code that reads it in `crates/tetsu`:

  ```toml
  format = 1

  [tenant]
  slug = "sample"
  name = "Sample Studio"

  [tenant.theme]
  primary = "#3A4250"
  accent = "#E5622B"

  [[tenant.locations]]
  name = "Main Street"
  time_zone = "America/Denver"
  ```

  Unknown fields, duplicate keys, and unknown versions fail. The reader checks `format` before the rest, so an
  old binary reports a newer file as a newer version, not a typo. Adding an optional field keeps the version.
  Any other change raises it, and Tetsu keeps reading the previous version until the demos repository moves.
- **We adopt `serde` and `toml`** for this, after the check in
  [Seed file format](../research/seed-format.md). `serde` derives stay on the file types in `crates/tetsu`, so
  the file format and the domain change separately.
- **One shared demo environment holds every demo,** which is the environment ADR-0012 already runs. Real
  businesses never share an environment with demos: they get their own when Tetsu moves to paid.
- **Demo marking applies to the whole environment.** `TETSU_DEMO` must be set to `true` or `false`, with no
  default, and `tetsu serve` refuses to start without it. When it is `true`, every page says the business is a
  demo and asks search engines not to index it.

## Alternatives considered

- **Seed files in Tetsu's repository.** One repository and one CI, and tests could read the files directly. But
  each new demo becomes a Tetsu commit, business data sits beside the code that must never name it, and demos
  could not move to the public API without changing where they live.
- **A separate deployment for each demo.** The strongest isolation, but Render's 750 free hours are shared across
  the workspace and Neon gives 100 CU-hours a month ([Hosting research](../research/hosting.md)), so a few demos
  would pass the free limits. It also stops demos from testing many tenants on one Tetsu.
- **A demo flag on each tenant.** Lets demos and real businesses share an environment, which is what Stripe moved
  away from, and a wrong flag would show a fictional business as real.
- **Seeds as Rust code or SQL.** Rust code names a business in the binary. SQL skips the domain's checks.
- **JSON or YAML.** JSON adds no crate but has no comments and is harder to edit by hand. The common YAML crate
  is archived. Details are in the research page.

## Consequences

Adding a demo needs no Tetsu change. Tetsu's repository names no business outside its docs, and the file format
becomes a contract we version. CI in the demos repository needs the published image, so it waits for the deploy
slice of #19.

**Harder.** A second repository holds a database secret for the demo environment until #12. It is kept as narrow
as Neon allows, and [#31](https://github.com/heavymedl-labs/tetsu/issues/31) sets credential rules. Raising the
format version means updating the demo files soon after. Skipping existing tenants means an edited file does not
change a tenant already seeded, so changing a live demo needs an update use case or a reset, decided when first
needed.

**Weakest evidence.** The Stripe example is one vendor's product change, read from its docs, not a study. The
crate health data was read once. The scenario tests the [reference businesses page](../reference-businesses/README.md)
expects cannot read files from a private repository in Tetsu's CI without a token across repositories. The first
scenario test decides between such a token and test data of its own.

**Revisit** when a real business needs a demo copy of its own data, or when the public API can create tenants.

## Sources

- [ADR-0010](0010-evaluating-technology.md) and [Seed file format](../research/seed-format.md).
- AWS SaaS Lens, "Tenant onboarding": automate tenant creation through one repeatable path.
- Stripe documentation, "Sandboxes": isolated test environments, separate from live data.
- TOML v1.1.0 specification, toml.io: duplicate keys are invalid.
- Serde documentation, "Container attributes": `deny_unknown_fields`.
