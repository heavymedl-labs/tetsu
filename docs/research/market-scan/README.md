# Vertical SaaS Market Scan

Last reviewed 2026-11-01 by Kurt Medley. Prices, ownership and features change quickly, so recheck a figure before relying on it. Copied from the Claude Docs original on 2026-11-02; this repository copy is now the source of truth.

A scan of vertical software markets, used to shape Tetsu (working name), our domain-agnostic Rust SaaS platform. Each vertical gets a folder here; what repeats across verticals belongs in the platform.

This is research, not a decision. Where it disagrees with an ADR, the ADR wins.

## Boutique fitness

Seven studio-software platforms, scanned for our first vertical. The market is consolidating, the loudest complaints are about business model rather than features, and no one pairs an open API with reliable engineering. That leaves room for an independent, engineering-led entrant.

### Findings

1. **Consolidation.** Xplor owns Mariana Tek and Momence, ABC Fitness owns Glofox, and Mindbody's parent Playlist merged with EGYM in March 2026. Worse support after an acquisition is the most consistent complaint (Mindbody, Glofox, Momence).
2. **Table stakes are rising.** Classes, appointments, waitlists, memberships, packs, POS, reporting and ClassPass are universal, and Mindbody includes AI churn insights at $79.
3. **All-in-one keeps widening:** events and trainings, native on-demand video, and built-in accounting and payroll tax filing (Arketa).
4. **Business model is the battleground:** processor lock-in (Mariana Tek, WellnessLiving), fee stacking (Momence), quote-only pricing, mid-contract increases and exit fees.
5. **No API does it all.** Glofox has the best webhooks, Momence self-serve OAuth, WellnessLiving a dogfooded spec. None combines self-serve keys, a sandbox, machine-to-machine credentials and reliable webhooks.

### What it means for us

- **Win on trust:** published flat pricing, a choice of payment processor, and customer data owned by the studio.
- **Match the table stakes, resist the sprawl:** build the roughly 20 capabilities every player has, and integrate payroll, accounting and video rather than building them.

Switching, the API and processor choice are covered in [Platform implications](platform-implications.md), which lists proposed platform changes from every vertical. Tables are in [Compare](boutique-fitness/compare.md) and each player's detail under [Boutique fitness](boutique-fitness/README.md).

## Method

- **Sources:** vendor help centers, feature pages, developer docs and press releases first; then review sites and third-party trackers.
- **"Not found":** the research didn't confirm the feature. It doesn't mean the product lacks it.
- **Reported figures:** spend, contract terms and fees that come from reviewers are marked as reported. Treat them as signals, not facts.

### Reading the feature maps

Every competitor profile has a feature map. Each row has a **Tag** (how we'd build it) and a **Platform capability** (where it would live). Capabilities and their patterns are listed in [Platform implications](platform-implications.md); role terms are in the [glossary](../../GLOSSARY.md).

| Tag | Meaning |
| --- | --- |
| Platform | Generic; built once in the platform |
| Platform + config | Platform feature each tenant or vertical configures |
| Vertical | Specific to one vertical; a module built on the platform capabilities listed |
| Integration | A third-party service reached through a driven port |
| (new) | Not in our plan yet |

A row whose capability is **None** has no platform home: it is vertical-only or outside our scope.

### Boutique fitness caveats

- **Uneven depth:** Mindbody's and Walla's help centers aren't public, so those maps lean on feature pages, blog posts and reviews. Walla's is the thinnest.
- **Competitor-written sources:** Vibefam, Arketa, Pabau and Supergood. They are flagged where used.
- **Not yet done:** interviews with studio owners. That is the main gap before scoping v1.
