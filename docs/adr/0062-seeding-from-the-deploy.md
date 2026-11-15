# 0062. Seeding the demo from Tetsu's deploy

- Status: Proposed
- Date: 2026-11-15
- Amends [ADR-0018](0018-demo-businesses-as-data.md): who applies the demo businesses

## Context

ADR-0018 keeps the demo businesses as data files in the private `heavymedl-labs/tetsu-demos` repository, and has
that repository's CI apply them ([#73](https://github.com/heavymedl-labs/tetsu/issues/73)). Until it did, the
maintainer seeded by hand, pasting the application role's URL from Render. A deploy that resets the demo
([#135](https://github.com/heavymedl-labs/tetsu/issues/135)) then leaves it without businesses until someone does.

The demos repository's CI would need the application role's URL as a secret of its own, and a token in Tetsu's
repository to start it after each deploy. Tetsu's deploy workflow already reads that URL from Render for its chores,
and is the one workflow allowed secrets ([ADR-0027](0027-secrets-revised.md)).

## Decision

- **Every deploy that migrates seeds the demo** with the image it deploys, after the isolation check and before
  Render serves the image. Seeding skips businesses that exist, so a deploy that changes nothing changes no
  business, and one that resets the demo leaves it complete.
- **The deploy reads the demo files with a read-only token,** `DEMOS_READ_TOKEN`: a fine-grained GitHub token
  that reaches only `tetsu-demos`, with read access to its contents and nothing else, expiring yearly. It is
  used only in the step that clones the files, through the environment, as ADR-0027 asks.
- **It seeds as the application role,** whose URL it reads from the Render service and masks, as the chores do,
  so the URL is kept in one place.
- **The demos repository's CI only checks its files** (`tetsu seed --check`), which needs no secret. A new demo
  business reaches the demo with the next deploy, or a deploy run for it.

## Alternatives considered

- **The demos repository applies its files,** as ADR-0018 planned. A second repository holding a database URL,
  and a token in Tetsu's repository to start its run after each deploy: two credentials where this needs one.
- **The demo files in Tetsu's repository.** No token at all, but ADR-0018's reasons stand: business data would
  sit beside the code that must never name it.

## Consequences

- The maintainer creates `DEMOS_READ_TOKEN` once and renews it yearly; the renewals reminder
  ([#136](https://github.com/heavymedl-labs/tetsu/issues/136)) warns before it lapses.
- Seeding by hand leaves the runbook, and #73 narrows to the demos repository's check.
- Once the public API exists ([#12](https://github.com/heavymedl-labs/tetsu/issues/12)), seeding moves to it, as
  ADR-0018 plans.

## Sources

- [ADR-0018](0018-demo-businesses-as-data.md) and [ADR-0027](0027-secrets-revised.md).
- GitHub, [Managing your personal access tokens](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens),
  on fine-grained tokens limited to one repository.
