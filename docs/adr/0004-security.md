# 0004. Security

- Status: Accepted
- Date: 2026-11-03

## Context

Tetsu handles credentials and payments. Home-grown cryptography causes severe
vulnerabilities, and touching card data puts a system in full PCI DSS scope.

## Decision

- Cryptography uses vetted, maintained crates, each chosen through an ADR. We
  never implement primitives or protocols ourselves.
- The payment processor's hosted page or iframe fields collect card data. Card
  data never reaches Tetsu servers, logs, or databases; Tetsu stores only
  processor tokens.

## Alternatives considered

- **Handle card data directly.** Full PCI DSS scope for no product benefit.

## Consequences

Payment flows depend on the processor's tokenization. Which processors to
support is an open decision. Dependency vulnerability and license checks,
secrets handling, and a target OWASP ASVS level each get their own ADR when the
implementation phase opens.

## Sources

- OWASP Cryptographic Storage Cheat Sheet; ASVS 5.0, Cryptography chapter.
- PCI Security Standards Council, PCI DSS v4.0.1, SAQ A.
