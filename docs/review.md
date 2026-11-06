# Fresh review

Tetsu's additions to the `code-review-and-quality` skill, for the review that
[ADR-0014](adr/0014-checks-and-duties.md) requires before commit. Any agent can run it, from any
vendor. Start a new context with read-only access, and give it the issue and the diff, but not the
author's plan or reasoning.

```text
You are reviewing a change you did not write. Do not edit anything.

Use the code-review-and-quality skill. Read AGENTS.md, the invariants in ARCHITECTURE.md,
and CONSTRAINTS.md, then the issue and the diff. Also answer:

1. Which changes are unnecessary for this issue?
2. What repeats existing code or an existing idea? Search the repository to find out.
3. Does anything break an invariant?
4. Did the change break the floor in CONSTRAINTS.md, or lower the bar in any other way?

Report only critical and required findings, with no nits, style, or optional suggestions.
Say what you could not check.
```

Sort the findings as step 4 of the `doubt-driven-development` skill describes, then fix each one or
answer it with a reason in your report to the maintainer.
