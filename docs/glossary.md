# Glossary

The project's shared vocabulary. Use these terms in code, docs, issues, and
conversation. Add a term when it first matters; change one only with care,
because code will follow it.

**Accepted risk.** A security requirement Tetsu should meet but does not yet, recorded with the
maintainer's approval, an issue, and a fix-by date ([ADR-0029](adr/0029-security-target-revised.md)).

**Adapter.** Code at a module's edge. An inbound adapter (HTTP API,
server-rendered pages) calls the module's use cases. An outbound adapter
implements a driven port with a specific technology, such as Postgres or a
payment processor.

**Admin.** A person who works for Tetsu itself, not for a tenant, and logs in to Tetsu's own tools at `/admin`
([ADR-0040](adr/0040-identity-and-sign-in.md), [ADR-0085](adr/0085-admins-not-operators.md)). ADRs before ADR-0085
call them operators.

**ADR.** Architecture Decision Record: a short document in `docs/adr` that
records one significant decision, its alternatives, and its sources.

**Browser floor.** The oldest browsers Tetsu still works in, stated once in `.browserslistrc`. Above it pages are
functional; the current and previous versions are fully supported, with every enhancement
([ADR-0053](adr/0053-supported-browsers.md)).

**Cell.** One complete, independent deployment of Tetsu serving some of its
tenants, so a failure in one cell does not reach the others. Whether Tetsu uses
cells is [undecided](https://github.com/heavymedl-labs/tetsu/issues/8).

**Check.** An automated control that gives the same answer every time, such as a
test or a lint. Contrast with a duty. See
[ADR-0014](adr/0014-checks-and-duties.md).

**Client profile.** A person a tenant serves, with their bookings, waivers, and history. An account acts for its
own profile, and for others only through a relink, unless it created them itself; a walk-in or a guest is a
profile with no account ([ADR-0040](adr/0040-identity-and-sign-in.md), [ADR-0041](adr/0041-account-recovery.md)).

**Composition root.** The `tetsu` binary, which wires modules together. The
only place that knows every module.

**Connectors.** The group of platform modules that connect Tetsu to outside
systems, such as payments, notifications, and the public API. Not to be
confused with adapters, which sit at the edge of every module.

**Control plane.** The platform modules that run Tetsu itself, such as
tenancy, identity and access, and audit.

**Customer account.** An account a tenant owns, for one of its customers to log in. Email is unique within the
tenant, so one person at two tenants has two accounts ([ADR-0040](adr/0040-identity-and-sign-in.md)).

**Demo business.** A fictional tenant, kept as a data file in the `tetsu-demos`
repository and shown only in the demo environment, which never holds a real
business. See [ADR-0018](adr/0018-demo-businesses-as-data.md).

**Design token.** A named design value, such as a color or a spacing step, kept in one file and generated
into the `--tetsu-*` CSS properties every page uses. A tenant's theme is its token values
([ADR-0031](adr/0031-front-end-model-revised.md)).

**Domain event.** A record that something happened in a module, published for
other modules to react to. Past tense, such as `booking.cancelled_late`, and delivered through the module's
outbox ([ADR-0043](adr/0043-domain-event-delivery.md)).

**Duty.** Judgment an agent or person must carry out because no tool can, such as
simplifying a change. Listed in [AGENTS.md](../AGENTS.md#duties).

**Embed.** A frame of a Tetsu page placed in a business's own website by the `<tetsu-embed>` element, for
browsing only: nothing logged in is ever framed ([ADR-0031](adr/0031-front-end-model-revised.md)).

**Fresh review.** A review by an agent that did not write the change, seeing only
the issue and the diff. See [docs/review.md](review.md).

**Global table.** A table whose rows belong to no one tenant, such as the address lookup, staff identities, or
sessions. Each says why it is global ([ADR-0042](adr/0042-tenant-isolation.md)).

**Guest.** A person who books with a tenant without an account, with a client profile of their own. A guest's
link can view or cancel that one booking ([ADR-0040](adr/0040-identity-and-sign-in.md)).

**Invite link.** A single-use link, good for an hour, through which an admin joins Tetsu with their first passkey.
The admin command issues it and prints it only to the terminal that ran it ([ADR-0040](adr/0040-identity-and-sign-in.md)).

**Island.** A small Web Component that adds behavior to server-rendered HTML on a Tetsu page, without
rendering markup of its own ([ADR-0031](adr/0031-front-end-model-revised.md)).

**Join.** What an invited admin does: open their invite link and create their first passkey, which logs them in
with a session that is never fresh ([ADR-0069](adr/0069-logged-in-on-joining.md)). People who make their own
account sign up instead.

**Kernel.** The small core that defines the module contract: traits and types,
with no domain knowledge and no runtime plug-in loading.

**Log in.** Starting a session with a passkey; the noun is login, and ending one is logging out. Tetsu never
says sign in, which reads too close to sign up.

**Minimum sellable product.** The least a reference business needs before it
switches to Tetsu. Each reference business's page defines its own, and going
live with it is a milestone.

**Module.** A Cargo crate that plugs into the kernel and owns at most one
Postgres schema. Platform capabilities and verticals are both modules.

**Notification address.** An extra email a person confirms, besides their account's own, for notices such as a
recovery ([ADR-0041](adr/0041-account-recovery.md)).

**Outbox.** A module's table of domain events, written in the same transaction as the change each describes, from
which a dispatcher delivers them at least once ([ADR-0043](adr/0043-domain-event-delivery.md)).

**Passkey.** A credential under the W3C WebAuthn standard, kept by a person's device or password manager and
bound to one domain. It counts as multi-factor when it verifies the user ([ADR-0040](adr/0040-identity-and-sign-in.md)).

**Platform.** Everything that is not a vertical: the kernel, the control plane,
shared capabilities, and connectors.

**Port.** A Rust trait a module calls out through, also called a driven port.
Outbound adapters implement it.

**Realm.** One of the three kinds of account, each with its own passkey domain: admin accounts, staff
members' identities, and customer accounts ([ADR-0040](adr/0040-identity-and-sign-in.md)).

**Recovery code.** A one-time secret a person saves, required for staff and owners and offered to customers,
which with an email code lets them enroll a new passkey on their account ([ADR-0041](adr/0041-account-recovery.md)).

**Reference business.** A fictional business that adopts Tetsu, used to check
decisions, milestones, and tests from a customer's side. There is one for each
industry in ADR-0002, described in
[reference-businesses](reference-businesses/README.md).

**Relink.** Giving an account the right to act for a client profile that has history or another account, after a
wait and notices ([ADR-0041](adr/0041-account-recovery.md)).

**Retired account.** An account disabled for good, its sessions ended, passkeys removed, and email freed, as
when a hijacked account is replaced ([ADR-0041](adr/0041-account-recovery.md)).

**Rigor tier.** The testing and review bar for an area, from 1 (highest) to 3,
set by [ADR-0005](adr/0005-testing-and-performance.md).

**Settings shell.** The layout every realm's account pages share, after Linear's settings mode: a sidebar of
the realm's pages beside a panel, and below a tablet's width a bar whose "Menu" opens the sidebar as a popover
(`web/templates/settings.html`, #156).

**Shared capability.** Industry-neutral behavior the platform builds up front
for more than one vertical, such as scheduling, booking, or the ledger.

**Sign up.** Making an account for yourself, as a customer will. Admins join from an invite instead.

**Staff member.** A person who works for one or more tenants, with one identity and a staff role at each ([ADR-0040](adr/0040-identity-and-sign-in.md)).

**Staff role.** A staff member's role at one tenant: owner, manager, front desk, or instructor
([ADR-0048](adr/0048-sign-in-design.md)).

**Surface.** A way people reach Tetsu: booking pages in a browser, hosted or embedded,
the staff app, native apps, or the public API. Every surface uses the same use cases.

**Tenant.** A business using Tetsu, such as a boutique fitness brand,
together with all of its locations. Tenant data is isolated from every
other tenant's.

**Tenant-owned table.** A table whose every row carries `tenant_id`, enforced by forced row-level security
([ADR-0042](adr/0042-tenant-isolation.md)).

**Tenant transaction.** The only way code reaches tenant-owned tables: a transaction that first sets the module
role, the tenant, and timeouts with transaction scope ([ADR-0042](adr/0042-tenant-isolation.md)).

**Use case.** An operation a module offers. Other modules and inbound adapters
call it; together with domain events, use cases are how modules talk.

**Vertical.** A module for one industry. It holds only what is specific to
that industry and builds on the platform. Boutique fitness is the first;
salons and spas are the second industry every capability is designed against.

**Walking skeleton.** The first thin slice through every layer, from login
to production, built to prove the architecture before feature work. It is the
first milestone.
