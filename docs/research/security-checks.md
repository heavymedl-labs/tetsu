# Security checks research

Research for [#31](https://github.com/heavymedl-labs/tetsu/issues/31), following
[ADR-0010](../adr/0010-evaluating-technology.md). This is research, not a decision. Where it
disagrees with an ADR, the ADR wins. It feeds [ADR-0024](../adr/0024-secrets.md),
[ADR-0025](../adr/0025-dependency-checks.md), and [ADR-0026](../adr/0026-security-target.md), and the
[review](#review-on-2026-11-08) below feeds their revisions, ADR-0027 to ADR-0029.

## Criteria

Written 2026-11-08, before looking at results.

| # | Criterion | Why |
| --- | --- | --- |
| M1 | Free on GitHub Free with a private repository | The organization's plan ([#70](https://github.com/heavymedl-labs/tetsu/issues/70)) |
| M2 | Runs from `scripts/check.sh`, so local and CI checks match | [ADR-0014](../adr/0014-checks-and-duties.md) |
| M3 | A secret scanner covers staged changes and all of git history | A secret removed from the tree stays in history |
| M4 | Scans never send code or findings off the machine | The repository is private |
| M5 | Dependency checks cover advisories, licenses, and sources | [ADR-0004](../adr/0004-security.md), [ADR-0011](../adr/0011-server-stack-guardrails.md) |
| W1 | Written in Rust and installed with `cargo install`, like our other tools | Maintainer preference: one toolchain |
| W2 | Cheap in CI minutes | [ADR-0023](../adr/0023-ci-on-demand.md) |
| W3 | Healthy project, as ADR-0010 step 4 defines | Checks we depend on must keep pace with new secret formats and advisories |

These are tooling choices, cheap to replace, so this follows the short process (steps 1, 4, and 7),
plus community practice and failure tests because the maintainer asked for them. Benchmarks do not
apply: scan time is under a second at our size.

## Findings

**Kingfisher for secrets and cargo-deny for dependencies.** Both met every must. Kingfisher is not
the common choice, and that is the weakest part of this page.

### Secret scanners

Read 2026-11-08 from GitHub, crates.io, and each project's README.

| Tool | Language, license | Latest release | Main maintainer's share | Scans history | Notes |
| --- | --- | --- | --- | --- | --- |
| Kingfisher | Rust, Apache-2.0 | v2.17.0, 2026-11-05, weekly | ~94 of the last 100 commits, at MongoDB | Yes | 134 issues, none open. Needs Rust 1.99, the version we pin |
| Betterleaks | Go, MIT | v1.10.0 and v2.0.0-rc.7, 2026-11-04 | Gitleaks' original author | Yes | Started 2026-02. Funded by Aikido Security. v2 not yet stable |
| Gitleaks | Go, MIT | v8.30.1, 2026-03-21 | Same author | Yes | README: "feature complete", security patches only |
| TruffleHog | Go, AGPL-3.0 | v3.99.8 | Truffle Security | Yes | Built to check whether keys still work; heavier |
| ripsecrets | Rust, MIT | v0.1.11, 2025-05-27 | One author | No | Fewer rules |
| Nosey Parker | Rust, Apache-2.0 | v0.24.0, 2025-05-08 | Praetorian | Yes | Archived |
| GitHub secret scanning | Hosted | n/a | GitHub | Yes | Paid on private repositories, $19 per active committer a month. Fails M1 |

Kingfisher ships Betterleaks' rule set and refreshes it nearly every release. Its changelog shows
v2.0.0-rc.7 pulled in this month. So it detects what the Gitleaks lineage detects, plus its own
rules: 502 loaded in our run. jaz, leaktor, and vsec are Rust scanners too new or too small to pass
step 4.

**Community practice.** Strength: moderate for what teams run, weak for detection quality.

- GitHub code search, 2026-11-08, counted workflow files naming each tool: Gitleaks about 59,000,
  TruffleHog about 11,000, Betterleaks about 625, Kingfisher 23 to 43.
- The common setup is layered: a scan before commit, a scan in CI that catches skipped hooks, and a
  history scan ([OWASP DevSecOps Guideline](https://owasp.org/www-project-devsecops-guideline/latest/01a-Secrets-Management)).
  No standard names a tool.
- rust-lang, tokio, astral-sh, Bytecode Alliance, and Embark run no secret scanner in CI. Their
  repositories are public, where GitHub scans for free, so their practice does not transfer.
- One small independent test planted ten secrets: Kingfisher and Gitleaks found 9 each, TruffleHog 10
  ([DevToolLab](https://devtoollab.com/blog/best-secret-scanning-tools)). MongoDB's own
  [comparison](https://github.com/mongodb/kingfisher/blob/main/docs/COMPARISON.md) reports Kingfisher 2
  to 10 times faster than both. That is a vendor claim. Most other comparisons found were vendor or
  SEO blogs.

**Failure tests.** Run 2026-11-08 on Linux, once, with Kingfisher 2.17.0 against this repository
at 116 commits.

| Case | Result |
| --- | --- |
| Full history, `--no-validate` | 0.3 s with ignored build output excluded. Without `--exclude target` it scanned build files, 7 s, and flagged a sample key inside a compiled crate |
| Our own code, before a baseline | Flagged test fixtures only: `s3cret-pw`, `hunter2`, and `sslmode` in connection strings. No real secrets |
| Baseline of those fixtures | Clean scan, exit 0. The baseline stores fingerprints, not secret text |
| Fake GitHub token and a passworded Postgres URL, staged in a throwaway clone | Both caught by `--staged`, exit 200 |
| The same file committed, then deleted | Still caught by the history scan, exit 200 |
| Networking removed with `unshare -n` | Same result. With `--no-validate --no-update-check` it needs no network |

**Install cost.** `cargo install --locked kingfisher-bin` took 9 min 20 s, 37 min of CPU, and 3.7 GB
of build space on the maintainer's machine, and filled a tmpfs `/tmp`. It builds the AWS SDK, a
MongoDB driver, and SQLite for features we do not use. CI's tool cache is keyed on `Cargo.lock`, so
building it there would repeat after each dependency change. Each release instead publishes a
`kingfisher-linux-x64.tgz` with a SHA-256 digest and in-toto provenance, which CI can download and verify.

### Dependency checks

| Tool | License | Latest release | Covers | Recent downloads |
| --- | --- | --- | --- | --- |
| cargo-deny | MIT or Apache-2.0 | 0.20.2, 2026-07-09 | Advisories, licenses, bans, sources | 2.3 million |
| cargo-audit | Apache-2.0 or MIT | 0.22.2, 2026-06-05 | Advisories | 4.3 million |
| Dependabot alerts | Hosted, free on private repositories | n/a | Advisories, between pushes | n/a |

cargo-deny's main maintainer wrote 489 of its commits. It has 86 open issues and pull requests, and needs
Rust 1.88. A CVSS 4.0 parsing failure ([#804](https://github.com/EmbarkStudios/cargo-deny/issues/804))
is fixed. rust-lang, tokio, astral-sh, Bytecode Alliance, and Embark all run it in CI.

**Failure test.** cargo-deny 0.20.2 installed in 51 s and checked the workspace in 5 s on
2026-11-08, with a draft config allowing permissive licenses and crates.io only.

- **Five advisories and three unmaintained crates, all through the benchmark harness.**
  RUSTSEC-2026-0446, -0447, -0448, and -0455 in actix-web and actix-http, RUSTSEC-2026-0258 in h2,
  and basic-toml, fxhash, and pin-utils unmaintained. actix-web is a comparison server in
  `benchmarks`. No crate we ship depends on any of them, checked with `cargo tree -i` per crate.
  Fixed versions exist ([#80](https://github.com/heavymedl-labs/tetsu/issues/80)).
- **Our own crates have no license field,** and path dependencies count as wildcards. Both need
  settings, not fixes.
- 37 duplicate-version warnings, mostly Windows target crates.

### Security target

[OWASP ASVS 5.0](https://github.com/OWASP/ASVS/blob/master/5.0/en/0x03-What-is-the-ASVS.md),
released 2025-05-30, read 2026-11-08:

- Level 1 is about 20% of the requirements, "the minimum requirements to consider".
- Level 2: "Most applications should be striving to achieve this level", about 70% of requirements in all.
- Level 3 is for "applications looking to demonstrate the highest levels of security".
- An organization picks a level by risk. "An early-stage startup that is only collecting limited
  sensitive data may decide to focus on Level 1", while a bank needs Level 3.

Tetsu will hold several businesses' customer data, staff accounts, and payment records, though card
data stays with the processor ([ADR-0004](../adr/0004-security.md)).

## Review on 2026-11-08

A fresh-context adversarial review of ADR-0024 to ADR-0026, the same day they were accepted. Each
finding below was reproduced or checked against its source. Findings above this section stand, except
where this section corrects them.

### Review: secret scanning

Run with Kingfisher 2.17.0 in throwaway repositories and a `--no-local` clone of tetsu.

| Case | Result |
| --- | --- |
| Neon-format URL in a `tests/` path, default confidence | Missed, exit 0. Kingfisher lowers test paths to `low` |
| The same, `--confidence low` | Caught, exit 200 |
| A token staged, then unstaged and deleted, full scan of `.` | Caught from an unreachable local object, exit 200 |
| The same, `--branch HEAD` | Not reported. Scans only reachable history |
| `# kingfisher:ignore` beside a token | Silenced by default. Reported with `--no-ignore` |
| `--skip-word fake-` | Also hid a real password when `fake-` was in the database name |
| `--skip-regex '^fake-[a-z0-9-]+$'` | Skipped `fake-pw-1`. Still caught a real password next to a `fake-` word |
| URL with host `example.com` | Never reported, even with no skip settings |

- The earlier `s3cret-pw` finding came from an unreachable local object. A clean clone flags the
  fixtures in `crates/postgres` and `crates/tetsu` only.
- Baseline fingerprints include file offsets, per Kingfisher's baseline docs, so an edit above a
  fixture makes it new again.
- `kingfisher-bin` 2.17.0 is on PyPI as prebuilt wheels for Linux, macOS, and Windows, with 67,119
  downloads in the last month (pypistats.org). It installs with uv in seconds, and reports the same
  version as the cargo build. The crates.io package has 114 downloads in total. Kingfisher has no
  published security advisories.
- On GitHub Free, environment secrets are only for public repositories, per GitHub's docs.
- On crates.io, the name `kingfisher` is an unrelated project.

### Review: dependency checks

- webpki-roots 1.0.11, a dependency of `crates/postgres`, is `CDLA-Permissive-2.0`.
- Our crates already inherit `publish = false`. cargo-deny flags them until `private.ignore` is set.
- actix-http 3.20.5, the newest, requires `h2 ^0.3.27`. RUSTSEC-2026-0258 is fixed only in h2 0.4.16.
- basic-toml comes through Askama, the chosen HTML engine, and has no safe upgrade. fxhash and
  pin-utils come through monoio in the harness.
- With targets set to Linux and macOS, `private.ignore`, and CDLA-Permissive-2.0 allowed, licenses,
  bans, and sources pass. Advisories fail only on the harness crates above, and duplicate warnings drop
  from 37 to 25.

### Review: security target

From the ASVS 5.0 chapters:

- V6.3: "L2 applications must force the use of multi-factor authentication." 6.3.3 (L2) adds that
  relaxing it "requires a fully documented rationale and a comprehensive set of mitigating controls".
- 6.2.12 (L2): breached-password checks. 6.3.1 (L1): brute-force controls. 6.3.5 and 6.3.7 (L3):
  notify users of suspicious sign-ins and of changed credentials.
- 8.4.1 (L2): cross-tenant controls in multi-tenant applications. 8.2.2 (L1): object-level access.
- Meeting a level means meeting all requirements of the levels below it. A minor ASVS release needs
  compliance re-checked; a patch release does not. IDs are best cited as `v<version>-<id>`.

## What this cannot show

The scanner tests ran once, on one small repository, with planted secrets in common formats.
Detection of unusual formats is not measured. Adoption counts come from code search, which samples
rather than counts every repository.
