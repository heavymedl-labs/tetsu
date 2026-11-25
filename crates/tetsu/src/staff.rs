//! The staff command (ADR-0089): invites a business's first owner, with `tetsu staff invite <business> <name>
//! <email> --role owner`. The composition root finds the business through Tenancy, and Identity, which never reads
//! Tenancy's tables, invites the person under that business's ID. The link is printed only to a terminal and emailed
//! only to the invited address, as an admin's is.

use tetsu_identity::{
    Accounts, BusinessId, InviteToken, NewStaffInvite, PersonName, Role, StaffDirectory,
    StaffInvite, staff_invite_message,
};
use tetsu_kernel::{Address, Mailer};
use tetsu_tenancy::{Slug, TenantStore, Tenants};

use crate::operator::{self, Acting};
use crate::settings::{self, Local};

/// A staff command, with what it acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum Staff {
    Invite {
        business: Slug,
        name: PersonName,
        email: Address,
        role: Role,
    },
}

const USAGE: &str = "staff takes invite <business> <name> <email> --role owner";

/// The staff command its words name, with its arguments checked.
pub fn parse(rest: &[String]) -> Result<Staff, String> {
    let [command, business, name, email, flag, role] = rest else {
        return Err(USAGE.into());
    };
    if command != "invite" || flag != "--role" || name.trim_start().starts_with('-') {
        return Err(USAGE.into());
    }
    Ok(Staff::Invite {
        business: Slug::parse(business).map_err(|error| error.to_string())?,
        name: PersonName::parse(name).map_err(|error| error.to_string())?,
        email: Address::parse(email).ok_or(format!("{email:?} is not a plain email address"))?,
        role: Role::parse(role).ok_or(format!(
            "{role:?} is not a role staff can be invited to yet; only owner is, until the other roles come"
        ))?,
    })
}

/// Runs a staff command with the settings `local` loads, and returns the lines it reports. Every staff command
/// issues a link, so `terminal` must be true, checked before any setting is read.
pub async fn run(
    staff: Staff,
    local: impl FnOnce() -> Result<Local, String>,
    terminal: bool,
) -> Result<Vec<String>, String> {
    if !terminal {
        return Err(operator::TERMINAL_ONLY.into());
    }
    let local = local()?;
    let acting = Acting {
        by: operator::who(),
        origin: operator::link_origin(&local, settings::STAFF_ORIGIN)?,
    };
    let (smtp, allow) = crate::mail(&local)?;
    let pool = crate::connect(&local).await?;
    let office = tetsu_mail::post_office(pool.clone(), smtp, allow);
    let tenants = tetsu_tenancy::postgres_store(pool.clone());
    let accounts = tetsu_identity::postgres_staff_store(pool);
    let Staff::Invite {
        business,
        name,
        email,
        role,
    } = staff;
    let (id, called) = found(&tenants, &business).await?;
    let invite = NewStaffInvite {
        business: id,
        name,
        email,
        role,
    };
    invited(&accounts, &office, &invite, &called, &acting).await
}

/// The business at a slug's address: its ID, for Identity, and its name, for the email.
async fn found(
    tenants: &Tenants<impl TenantStore>,
    slug: &Slug,
) -> Result<(BusinessId, String), String> {
    match tenants.resolve(&slug.address()).await {
        Ok(tenant) => Ok((
            BusinessId::new(tenant.id().get()),
            tenant.name().to_string(),
        )),
        Err(tetsu_tenancy::Error::NotFound) => Err(format!("no business at {}", slug.address())),
        Err(error) => Err(error.to_string()),
    }
}

/// Invites a person to a business called `called` through Identity's use cases, prints the link, and emails it,
/// saying what came of each.
async fn invited(
    accounts: &Accounts<impl StaffDirectory>,
    mailer: &impl Mailer,
    invite: &NewStaffInvite,
    called: &str,
    acting: &Acting,
) -> Result<Vec<String>, String> {
    let issued = accounts
        .invite_staff(invite, &acting.by)
        .await
        .map_err(|error| error.to_string())?;
    let url = operator::link_url(&acting.origin, "/app/join", &issued.token);
    let mut said = whom(&issued, invite, called);
    said.push(format!("invite link, good once and for seven days: {url}"));
    said.push(emailed(mailer, invite, called, &url, &issued.token).await);
    Ok(said)
}

/// Whom an invite made or found, and whether they keep a name other than the one given.
fn whom(issued: &StaffInvite, invite: &NewStaffInvite, called: &str) -> Vec<String> {
    let staff = issued.staff.get();
    let role = invite.role.as_str();
    let mut said = vec![if issued.created {
        format!(
            "created staff member {staff}, {}, and invited them to {called} as {role}",
            invite.name.as_str()
        )
    } else {
        format!("invited staff member {staff} to {called} as {role}")
    }];
    if issued.renamed {
        said.push(format!(
            "staff member {staff} keeps the name they have, which only they change"
        ));
    }
    said
}

/// What came of emailing an invite's link to the address it invites.
async fn emailed(
    mailer: &impl Mailer,
    invite: &NewStaffInvite,
    called: &str,
    url: &str,
    token: &InviteToken,
) -> String {
    let to = &invite.email;
    match staff_invite_message(to.clone(), called, invite.role, url, token) {
        Some(message) => operator::delivered(to, mailer.send(&message).await),
        None => format!(
            "could not write the invite email for {}; the link above still works",
            to.as_str()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args;
    use tetsu_identity::Actor;
    use tetsu_kernel::Mailbox;
    use tetsu_mail::PostOffice;

    fn inviting(business: &str, name: &str, email: &str) -> Staff {
        Staff::Invite {
            business: Slug::parse(business).unwrap(),
            name: PersonName::parse(name).unwrap(),
            email: Address::parse(email).unwrap(),
            role: Role::Owner,
        }
    }

    const WORDS: [&str; 6] = [
        "invite",
        "fern-studio",
        "Ada Park",
        "ada@example.com",
        "--role",
        "owner",
    ];

    #[test]
    fn staff_commands_are_parsed_exactly() {
        assert_eq!(
            parse(&args(&WORDS)),
            Ok(inviting("fern-studio", "Ada Park", "ada@example.com"))
        );
        let swapped = [
            "invite",
            "fern-studio",
            "Ada Park",
            "ada@example.com",
            "owner",
            "--role",
        ];
        let longer = [
            "invite",
            "fern-studio",
            "Ada Park",
            "ada@example.com",
            "--role",
            "owner",
            "x",
        ];
        for bad in [&[][..], &WORDS[..4], &WORDS[..5], &swapped, &longer] {
            assert_eq!(parse(&args(bad)), Err(USAGE.into()), "{bad:?}");
        }
        for (at, word) in [(0, "list"), (2, "--help")] {
            let mut changed = WORDS;
            changed[at] = word;
            assert_eq!(parse(&args(&changed)), Err(USAGE.into()), "{changed:?}");
        }
    }

    /// Each argument is checked, and a wrong one says why.
    #[test]
    fn each_argument_is_checked() {
        let with = |at: usize, word: &str| {
            let mut changed = WORDS;
            changed[at] = word;
            parse(&args(&changed)).unwrap_err()
        };
        assert!(with(1, "Fern Studio").starts_with("invalid: slug \"Fern Studio\" must be"));
        assert!(with(2, "").starts_with("invalid: a name has 1 to"));
        assert_eq!(
            with(3, "Ada <ada@example.com>"),
            "\"Ada <ada@example.com>\" is not a plain email address"
        );
        assert_eq!(
            with(5, "manager"),
            "\"manager\" is not a role staff can be invited to yet; only owner is, until the other roles come"
        );
    }

    /// A link is issued only to a terminal, checked before any setting is read.
    #[tokio::test]
    async fn a_link_is_issued_only_to_a_terminal() {
        let staff = inviting("fern-studio", "Ada Park", "ada@example.com");
        assert_eq!(
            run(staff, || panic!("reads no settings"), false).await,
            Err(operator::TERMINAL_ONLY.into())
        );
    }

    /// Without the site's origin there is nowhere for a link to point, so no one is invited.
    #[tokio::test]
    async fn a_link_needs_the_sites_origin() {
        let staff = inviting("fern-studio", "Ada Park", "ada@example.com");
        assert_eq!(
            run(staff, || Ok(Local::default()), true).await,
            Err("TETSU_STAFF_ORIGIN must be set".into())
        );
    }

    const FERN: BusinessId = BusinessId::new(3);

    fn acting() -> Acting {
        Acting {
            by: Actor::parse("kurt@workstation").unwrap(),
            origin: "https://tetsu.example".to_string(),
        }
    }

    async fn said(
        accounts: &Accounts<impl StaffDirectory>,
        office: &PostOffice,
        name: &str,
        email: &str,
    ) -> Result<Vec<String>, String> {
        let invite = NewStaffInvite {
            business: FERN,
            name: PersonName::parse(name).unwrap(),
            email: Address::parse(email).unwrap(),
            role: Role::Owner,
        };
        invited(accounts, office, &invite, "Fern Studio", &acting()).await
    }

    /// Inviting says who was made or found, prints the seven-day link to the staff join page, and never keeps the
    /// invite in the mailbox; a second invite to the same address under another name keeps the stored name.
    #[tokio::test]
    async fn an_invite_says_who_it_invites_and_prints_its_link() {
        let (pool, office, database) = operator::mailing().await;
        let accounts = tetsu_identity::postgres_staff_store(pool);
        let first = said(&accounts, &office, "Ada Park", "ada@example.com")
            .await
            .unwrap();
        assert_eq!(first.len(), 3, "{first:?}");
        assert_eq!(
            first[0],
            "created staff member 1, Ada Park, and invited them to Fern Studio as owner"
        );
        let token = first[1]
            .strip_prefix(
                "invite link, good once and for seven days: https://tetsu.example/app/join#",
            )
            .unwrap_or_else(|| panic!("{}", first[1]));
        assert!(
            token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()),
            "{token}"
        );
        assert!(first[2].starts_with("did not email the invite to ada@example.com, since"));
        let again = said(&accounts, &office, "Ada P", "ada@example.com")
            .await
            .unwrap();
        assert_eq!(again[0], "invited staff member 1 to Fern Studio as owner");
        assert_eq!(
            again[1],
            "staff member 1 keeps the name they have, which only they change"
        );
        assert_eq!(office.captured().await.unwrap(), vec![]);
        tetsu_postgres::connect(&database)
            .await
            .unwrap()
            .batch_execute(
                "INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name) VALUES (3, 1, 'owner')",
            )
            .await
            .unwrap();
        assert_eq!(
            said(&accounts, &office, "Ada Park", "ada@example.com").await,
            Err("invalid: the staff member at ada@example.com holds a role at this business already".into())
        );
    }

    /// A business is found by its slug's address, and an unknown one is named.
    #[tokio::test]
    async fn a_business_is_found_by_its_slug() {
        let database = tetsu_postgres::test_support::migrated_database(&[&tetsu_tenancy::Tenancy])
            .await
            .unwrap();
        let tenants =
            tetsu_tenancy::postgres_store(tetsu_postgres::pool(&database.as_app()).unwrap());
        let theme = tetsu_tenancy::Theme {
            primary: tetsu_tenancy::Color::parse("#112233").unwrap(),
            accent: tetsu_tenancy::Color::parse("#445566").unwrap(),
        };
        let fern = Slug::parse("fern-studio").unwrap();
        let tenant = tenants
            .create(
                &tetsu_tenancy::NewTenant::new(fern.clone(), "Fern Studio", theme, vec![]).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            found(&tenants, &fern).await,
            Ok((
                BusinessId::new(tenant.id().get()),
                "Fern Studio".to_string()
            ))
        );
        assert_eq!(
            found(&tenants, &Slug::parse("oak-yoga").unwrap()).await,
            Err("no business at /oak-yoga".into())
        );
    }
}
