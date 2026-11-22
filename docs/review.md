# Fresh review

Tetsu's additions to the `code-review-and-quality` skill, for the review that
[ADR-0014](adr/0014-checks-and-duties.md) requires before commit. Any agent can run it, from any
vendor. Start a new context with read-only access, and give it the issue and the diff, but not the
author's plan or reasoning.

**Doubt mode** folds in the `doubt-driven-development` skill
([ADR-0077](adr/0077-doubt-mode-in-the-fresh-review.md)). Use it for a Tier 1 change, one that crosses a module or
isolation boundary, one that changes trust (TLS, sessions, secrets, who may do what), and one the issue or the
author calls risky. Add the doubt block to the prompt, with the claims to doubt written as questions and none of
the author's reasons for believing them.

```text
You are reviewing a change you did not write. Do not edit anything.

Use the code-review-and-quality skill. Read AGENTS.md, docs/invariants.md,
and CONSTRAINTS.md, then the issue and the diff. Also answer:

1. Which changes are unnecessary for this issue?
2. What repeats existing code or an existing idea? Search the repository to find out.
3. Does anything break an invariant?
4. Did the change break the floor in CONSTRAINTS.md, or lower the bar in any other way?
5. Does every new promise, in code comments, docs, or an ADR, have a test or check that would fail if it broke,
   or a stated reason why not?
6. Does the change meet each ASVS requirement the issue names, and at least Level 1 of every ASVS
   chapter that applies to the code it changes (Level 2 for Tier 1 code)? Which apply but the issue
   left out? See ADR-0029.
7. For a page or component, compare it with its nearest Tailwind Plus counterpart, through the
   tailwind-plus skill's local library. Where does it fall short on spacing, type, behavior at each
   breakpoint, states, accessibility, or either color scheme? Does any of it copy Tailwind Plus
   markup? See ADR-0035.

8. For a change to SQL or migrations, check it against the supabase-postgres-best-practices
   skill's rules (ADR-0082). Where a rule disagrees with this repository, the repository wins.

Report only critical and required findings, with no nits, style, or optional suggestions.
Say what you could not check.
```

The doubt block, added after the prompt's last line:

```text
Doubt mode, using the doubt-driven-development skill. Assume the author is overconfident. Do not validate or
summarize. For each claim below, and for each property the change relies on that the compiler cannot check, such
as isolation, ordering, idempotence, timing, or trust, find the input, state, or setting that breaks it, with the
file and line. If you cannot after trying, say so plainly.

Claims to doubt:
- <a question, such as: can this setting weaken TLS in production?>
```

Sort the findings as step 4 of the `doubt-driven-development` skill describes, then fix each one or
answer it with a reason in your report to the maintainer. In doubt mode:

- **Loop on what you fix.** After fixing an actionable finding, review the new diff the same way, at most three
  times in all. Still finding substantive issues after three means the change is not ready: stop, and tell the
  maintainer.
- **Offer a second model.** With the maintainer present, offer the same prompt to another vendor's agent, such as
  Codex or Gemini through its command line in read-only mode, given only the prompt, the issue, and the diff. The
  maintainer decides; say in the report whether it ran. Without the maintainer, skip it and say so.
