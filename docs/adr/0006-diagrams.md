# 0006. Diagrams

- Status: Accepted
- Date: 2026-11-02

## Context

The docs need architecture and process diagrams that agents can write and
reviewers can diff. The one diagram today is ASCII art in `ARCHITECTURE.md`,
which is hard to edit and reads poorly.

## Decision

Diagrams are Mermaid code blocks in the Markdown file that owns them. GitHub
renders them natively, so there is no build step, tool to install, or
generated file to commit.

## Alternatives considered

- **D2 with committed SVGs.** Better layouts and portable files, but every
  change needs a pinned renderer, a regenerated SVG, and a CI drift check.
- **PlantUML.** Deepest UML support, but needs Java and a render step, and its
  output looks dated.
- **Structurizr DSL.** One C4 model with many views, but a steep learning
  curve, and its tooling is being reorganized.
- **Excalidraw or draw.io.** Easy to draw by hand, but the files diff poorly
  and agents cannot edit them reliably.

## Consequences

Diagrams change in reviewable text with nothing to maintain. Layout is
Mermaid's to decide, so large diagrams can read poorly; split them instead.
Diagrams render on GitHub only, so a future docs or marketing site needs its
own Mermaid renderer.

## Sources

- GitHub Docs, "Creating diagrams": native Mermaid rendering in Markdown.
- Mermaid documentation, mermaid.js.org: flowchart, sequence, and state
  diagram syntax.
