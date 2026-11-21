# Security testing tools research

Research for [#150](https://github.com/heavymedl-labs/tetsu/issues/150) and the guide to
[testing with an agent](../security/testing-with-an-agent.md), gathered 2026-11-21. This is research, not a decision:
running a tool on a tester's own machine needs no ADR, but the project adopting one, in the repository or CI, does
([ADR-0010](../adr/0010-evaluating-technology.md)). Most sources are vendor posts and roundups, so rankings and
benchmark scores are unverified.

## In the guide

- **A coding agent reading the code**, with the `security-and-hardening` skill, which covers Tetsu's small codebase
  in a bounded session.
- **OWASP ZAP**, free, in its `zaproxy/zap-stable` Docker image (about 4 GB): baseline and full scans of the running
  copy. Finds header and response issues, not denial of service. Its crawler reaches little from `/` alone, so the
  guide scans three starting points.

## Considered, not in the guide

| Tool | What it does | Why not yet |
| --- | --- | --- |
| Semgrep (open-source engine) | Static analysis against rule sets | A repeatable second opinion beside the agent's review; the strongest candidate to add later |
| `cargo-fuzz` | Millions of malformed inputs at a function | Would test Tier 1 parsers (cookies, addresses, invite tokens, seed files) as nothing else here does; project work needing an ADR |
| Nuclei | Ready-made checks for known products and misconfigurations | Aimed at known software; ZAP already flags the header kind |
| sqlmap | SQL injection | Tetsu only uses parameterized queries |
| ffuf | Guesses paths and parameters | Tetsu has few routes, and the guide lists them |
| testssl.sh | TLS configuration | The local copy is plain HTTP, and the demo is off limits |
| Burp Suite Community | A desktop proxy | Redundant with ZAP's desktop app |
| k6 | Scenario load tests | Denial of service and latency belong to [#37](https://github.com/heavymedl-labs/tetsu/issues/37), which has no budgets yet |

## Autonomous agents

Agents that run a whole test alone, each in Docker and needing an API key billed by use beyond a Claude
subscription. The guide names the first two as optional.

- **Shannon** (KeygraphHQ, AGPL-3.0): reads the source and tests in a browser. Reports the top white-box score on
  XBOW's 104-challenge web benchmark, though a competitor notes white-box runs inflate such scores.
- **Strix** (Apache 2.0): tools in a Docker sandbox, and a skill for coding agents such as Claude Code.
- **PentAGI, CAI, HexStrike AI:** other autonomous frameworks. **PentestGPT** guides a person step by step instead.
- **Burp Suite or ZAP through MCP connectors:** community-built, letting Claude Code read captured traffic and send
  requests. Treat one as privileged software, since a response can carry text that instructs the agent.

## Sources

- [AI Pentesting Agents 2026: 39+ Tools Tested (AppSec Santa)](https://appsecsanta.com/research/ai-pentesting-agents-2026)
- [Open source autonomous AI pentesting tools in 2026: an honest field guide (DEV)](https://dev.to/darkmoonx/open-source-autonomous-ai-pentesting-tools-in-2026-an-honest-field-guide-3ad0)
- [Strix on GitHub](https://github.com/usestrix/strix) and [Shannon on GitHub](https://github.com/KeygraphHQ/shannon)
- [Baselines Before Architecture: coding agents for pentesting (arXiv)](https://arxiv.org/pdf/2607.13085)
- [We Cleared the XBOW Benchmark Suite (Penetrify)](https://www.penetrify.cloud/en/blog/we-cleared-the-xbow-benchmark-suite-in-our-fastest-tier-clearing-xbow-benchmarks-fast-tier/)
- [AI-assisted web pentesting using Claude Code + Burp MCP (SecEngAI)](https://labs.secengai.com/p/ai-assisted-web-pentesting-using-claude-code-burp-mcp)
- [zap-baseline.py (zaproxy on GitHub)](https://github.com/zaproxy/zaproxy/blob/main/docker/zap-baseline.py)
