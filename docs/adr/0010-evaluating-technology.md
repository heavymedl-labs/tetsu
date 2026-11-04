# 0010. Evaluating technology

- Status: Accepted
- Date: 2026-11-04

## Context

[ADR-0001](0001-how-we-work.md) says adopting a framework, library, service, or
tool needs an ADR, but not what evidence that ADR needs.
[ADR-0008](0008-benchmark-harness.md) covers how to run a benchmark, not when to
run one or how much weight it carries. Coding agents write most of our ADRs, so
without a written bar the depth of each evaluation depends on who ran it.

The [server stack](0007-server-stack.md) evaluation shows why several kinds of
evidence are needed. A local benchmark showed a false tie that CI runs exposed as
a load generator limit. Community review and small failure tests then found a
migration deadlock and a statement cache failure that no benchmark had shown.

## Decision

Evaluate each adoption in these steps, and record the work in `docs/research`.
The full process applies when a choice is expensive to reverse or sits on a hot
path. For anything else, steps 1, 4, and 7 are enough. A step that cannot apply,
such as benchmarking a hosted service, is skipped with the reason stated.

1. **Set the criteria first.** Write down what the choice must do, using the
   [reference businesses](../reference-businesses/README.md) for real load and
   needs, before looking at results.
2. **List the options.** Start with maintained open source projects, plus a
   homegrown baseline where feasible. A proprietary option needs a stated reason.
3. **Benchmark.** Explore locally, then confirm on CI runners with the harness.
   Decisions cite the CI results. Report ratios with confidence intervals, check
   that the load generator is not the limit, and state what the setup cannot show.
4. **Check project health.** Record the license, security advisories, latest
   release, active contributors, how much depends on one maintainer, open issues,
   recent downloads, and minimum Rust version, each with the date it was read.
5. **Review community practice.** Find what teams use and what has gone wrong for
   them, and say how strong that evidence is. Departing from the common default
   needs a stated reason.
6. **Test how it fails.** Read the code where it matters, and run small tests of
   risky interactions and defaults, such as locking, schema changes, timeouts, and
   behavior under load.
7. **Weigh it all.** Before acceptance, review the evidence as a whole. No score
   or single result decides. The ADR states the tradeoffs, the weakest evidence,
   the cost of replacing the choice, and what would make us revisit it.

## Alternatives considered

- **A weighted scoring matrix.** Looks objective, but the weights are opinions,
  and a total hides a single disqualifying flaw.
- **Benchmarks alone.** Easy to compare, but they miss failure behavior, project
  risk, and whether the difference matters next to a database round trip.
- **Popularity alone.** Follows what most teams use, but gives no evidence for our
  workload, and common practice is mostly reported in blog posts.
- **No written process.** Keeps ADRs cheap, but depth would vary by author, and
  gaps would go unnoticed.
- **The full process for every dependency.** Consistent, but a small helper crate
  does not justify benchmarks.

## Consequences

Foundational choices take longer and have evidence we can recheck. Small
dependencies stay cheap. Evidence ages, so each research page is dated and an ADR
names its revisit triggers. The judgment in step 7 stays with the maintainer, who
accepts the ADR.

This applies to future decisions. The server stack evaluation came close, but its
community review ran after acceptance, and its research page records no license
or security advisory check.

## Sources

- [Server stack research](../research/server-stack.md): the evaluation this
  process comes from.
- Michael Nygard, "Documenting Architecture Decisions," Cognitect, 2011.
- Brendan Gregg, *Systems Performance*, 2nd ed., Addison-Wesley, 2020, ch. 12:
  benchmarking, including active benchmarking.
- OpenSSF Scorecard, securityscorecards.dev: signals of project health and risk.
- CHAOSS project, chaoss.community: metrics for open source community health.
