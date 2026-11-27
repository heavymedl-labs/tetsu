# Staff roles and the Team page

Researched 2026-11-27 for [#111](https://github.com/heavymedl-labs/tetsu/issues/111)'s sub-slice 4: the four staff
roles with their permissions, and inviting staff from the web. What is settled is not repeated here: the roles and
what each may do ([ADR-0048](../../adr/0048-sign-in-design.md)), invites and their links
([ADR-0089](../../adr/0089-staff-join-and-login.md)), the Team page's columns and the business settings group
([account area and dashboards](account-area-and-dashboards.md)), and realm paths at the root of their hosts
([ADR-0094](../../adr/0094-realm-paths-at-the-root-of-their-hosts.md)). Sources were read through search summaries on
2026-11-27, and say so where they are a third party rather than the vendor's documentation.

## Summary

Booking software for studios uses the same four roles as Tetsu under other names, and a business's people are managed
on one page, by the owner and, for the lower roles, by managers. Every product read stops escalation the same way: a
person grants only roles below their own, or only permissions they hold, and only an owner makes an owner. A business
always keeps an owner: the last one can be neither removed nor demoted, and no one changes their own role. Products
that put the business in the address check the person's role at that business on each request, so removing a role
ends that access at once.

## Studio software has the same four roles

**Takeaway:** Owner, manager, front desk, and instructor are the common shape; the names differ.

| Product | Roles | Source |
| --- | --- | --- |
| Glofox | Super Admin, Admin, Receptionist, Trainer; admins are upgraded or downgraded in the dashboard | [Glofox: staff and permissions](https://support.glofox.com/hc/en-us/articles/46467475896340); [Glofox product updates, June 2026](https://www.glofox.com/blog/abc-glofox-june-2026-product-updates/) |
| Mindbody | Permission groups, with Staff, Manager, and Owner; some permissions are the owner's alone | [Mindbody: staff management](https://www.mindbodyonline.com/business/staff-management); a third-party guide, [QuickerNotes](https://quickernotes.freshdesk.com/support/solutions/articles/153000145807-what-s-the-difference-between-a-mindbody-owner-staff-account-) |
| Momence | Default roles such as Front Desk and Teacher, plus custom roles | [Momence: staff accounts](https://help.momence.com/en/articles/12030073-staff-accounts-teachers-faq-s) |
| Booksy | Owner and Manager are the only roles that invite staff and assign permissions | [Booksy: staff permissions](https://support.booksy.com/hc/en-gb/articles/16535688940946-How-do-I-manage-staff-permissions) |
| Fresha | One account owner with full access; custom permission roles, edited only by the owner or those allowed to | [Fresha: permission roles](https://www.fresha.com/help-center/knowledge-base/team/100687-create-permission-roles) |

Custom roles (Fresha, Momence, Square) wait, as ADR-0048 decided, until a real business needs one.

## No one grants more than they hold

**Takeaway:** a person grants only roles below their own; only owners make owners.

- Shopify: staff who may edit permissions edit only "permissions that they already have", and only the owner
  transfers ownership ([Shopify Help: staff permissions](https://help.shopify.com/manual/your-account/staff-accounts/staff-permissions)).
- Mastodon: Manage Roles assigns only "roles below theirs", and cannot modify roles of equal or higher priority
  ([Mastodon docs: roles](https://docs.joinmastodon.org/admin/roles/)).
- GitLab: maintainers cannot invite or promote anyone to Owner, checked in the API too; removing an Owner needs the
  Owner role ([GitLab issue 439409](https://gitlab.com/gitlab-org/gitlab/-/issues/439409);
  [GitLab: project members](https://archives.docs.gitlab.com/16.7/ee/user/project/members)).
- Square: Manage Team Members assigns people to permission sets; creating sets is a separate permission
  ([Square: delegated team management](https://squareup.com/help/au/article/6170)).
- GitHub: only owners invite, cancel invites, and remove members
  ([GitHub Docs: roles](https://docs.github.com/en/organizations/managing-peoples-access-to-your-organization-with-roles/roles-in-an-organization)).

## A business always keeps an owner

**Takeaway:** the last owner can be neither removed nor demoted, and no one changes their own role.

- Vercel: a member cannot leave or be removed while the last Owner; promote another first. Invites carry a role and
  last 7 days; only owners invite through the API and change roles
  ([Vercel: managing team members](https://vercel.com/docs/rbac/managing-team-members)).
- GitHub: "You can't change your own role", and an organization should have at least two owners
  ([GitHub Docs: roles](https://docs.github.com/en/organizations/managing-peoples-access-to-your-organization-with-roles/roles-in-an-organization);
  [GitHub Docs: ownership continuity](https://docs.github.com/en/organizations/managing-peoples-access-to-your-organization-with-roles/maintaining-ownership-continuity-for-your-organization)).
- Fresha: one owner, transferred only to someone already on the team
  ([Fresha: transfer ownership](https://www.fresha.com/help-center/knowledge-base/workspace-settings/100665-transfer-workspace-ownership-access-1)).
- Linear: roles change from the Members page's row menu; removing a member deactivates them rather than deleting
  ([Linear: members and roles](https://linear.app/docs/members-roles)).

## The address names the business, and each request checks the role

**Takeaway:** a business's pages sit under its slug, and the role at that business is read on each request.

Linear's live settings carry the workspace in every address, `/<workspace>/settings/members`, and Vercel's team pages
sit at `vercel.com/<team>/...` ([account area and dashboards](account-area-and-dashboards.md)). Since the address, not
the session, names the business, the server reads the person's role there on each request, and a removed member is
refused on their next one. Stripe, the outlier, keeps the account in the session.

## What follows for Tetsu

- **Permissions in code, one per check,** with a fixed table from role to permissions, as ADR-0048 asks. Only checks
  that exist get a permission: for this sub-slice, seeing the team, inviting, changing a role, removing a member, and
  cancelling an invite.
- **A rank rule beside the table:** owner, then manager, then front desk and instructor. A person acts only on roles
  below their own and grants only those, except that an owner acts on and grants any role. That gives managers
  ADR-0048's "invite front desk and instructors" without a permission per role.
- **The last owner stays,** checked in the database under a lock, so two owners demoting each other at once cannot
  leave none. No one changes their own role or removes themselves here; leaving waits for a later slice.
- **Business pages at `/<business>/settings/team` on the staff host,** with the role read in that business's own
  transaction on each request, so removal ends access at once (`v5.0.0-7.4.2`) and a request never reaches another
  business (`v5.0.0-8.4.1`).
- **The answer to an invite says nothing from other businesses.** The store knows whether an email is disabled or
  keeps another name, which may come from another business (ADR-0089's clarification), so the page answers "Invite
  sent" for every invite it does not refuse for being a member here already.
