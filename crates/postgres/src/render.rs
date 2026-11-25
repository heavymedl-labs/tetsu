//! One source of SQL rendered for each realm that shares it (ADR-0091): a function written once for one realm is
//! rendered for another by swapping whole names, so no copy exists to drift.

/// Whether a byte can be part of a SQL name or a quoted name's text.
fn naming(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `found` starts at `at` in `text` as a whole name: with no naming byte just before or after it, so a swap
/// never changes part of a longer name.
fn whole(text: &str, at: usize, found: &str) -> bool {
    let bytes = text.as_bytes();
    let before = at.checked_sub(1).map(|index| bytes[index]);
    let after = bytes.get(at + found.len()).copied();
    let edge = |byte: Option<u8>| byte.is_none_or(|byte| !naming(byte));
    edge(before) && edge(after)
}

/// `source` with each whole name in `swaps` replaced by its pair, as `admin_sessions` by `staff_sessions` or
/// `'admin'` by `'staff'`. A name that is part of a longer one, such as `admins` in `admins_id_seq`, is left alone, so
/// each longer name needs a swap of its own. An empty name swaps nothing.
#[must_use]
pub fn render(source: &str, swaps: &[(&str, &str)]) -> String {
    let mut rendered = String::with_capacity(source.len());
    let mut rest = source;
    // Each step takes at least one character off what is left, so the scan always ends.
    while let Some(next) = rest.chars().next() {
        let at = source.len() - rest.len();
        let swap = swaps.iter().find(|(from, _)| {
            !from.is_empty() && rest.starts_with(from) && whole(source, at, from)
        });
        let (taken, after) = rest.split_at(swap.map_or(next.len_utf8(), |(from, _)| from.len()));
        rendered.push_str(swap.map_or(taken, |(_, to)| to));
        rest = after;
    }
    rendered
}

/// Each whole name of `forbidden` that `rendered` still holds, so a test can show a realm's rendered SQL names
/// nothing of the realm it was rendered from.
#[must_use]
pub fn leftovers<'a>(rendered: &str, forbidden: &[&'a str]) -> Vec<&'a str> {
    forbidden
        .iter()
        .copied()
        .filter(|name| {
            rendered
                .match_indices(name)
                .any(|(at, _)| whole(rendered, at, name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAFF: [(&str, &str); 4] = [
        ("admin_sessions", "staff_sessions"),
        ("admins", "staff"),
        ("admin_id", "staff_id"),
        ("'admin'", "'staff'"),
    ];

    /// Whole names are swapped, wherever they stand.
    #[test]
    fn whole_names_are_swapped() {
        assert_eq!(
            render(
                "DELETE FROM identity.admin_sessions AS s WHERE s.admin_id = $1 AND s.realm = 'admin';",
                &STAFF
            ),
            "DELETE FROM identity.staff_sessions AS s WHERE s.staff_id = $1 AND s.realm = 'staff';"
        );
        assert_eq!(render("admins", &STAFF), "staff");
        assert_eq!(render("(admins)", &STAFF), "(staff)");
    }

    /// Part of a longer name is never swapped, before or after, so a missing swap shows as a leftover, not a mangled
    /// name.
    #[test]
    fn part_of_a_longer_name_is_left_alone() {
        for kept in [
            "admins_id_seq",
            "readmins",
            "admin_idle",
            "my_admin_id",
            "'administrator'",
            "admins9",
        ] {
            assert_eq!(render(kept, &STAFF), kept, "{kept}");
        }
    }

    /// Text that is not ASCII passes through whole.
    #[test]
    fn other_text_passes_through() {
        assert_eq!(
            render("-- Tetsu admin · ünï admins", &STAFF),
            "-- Tetsu admin · ünï staff"
        );
        assert_eq!(render("", &STAFF), "");
    }

    /// An empty name swaps nothing, so the scan still ends.
    #[test]
    fn an_empty_name_swaps_nothing() {
        assert_eq!(
            render("a admins", &[("", "x"), ("admins", "staff")]),
            "a staff"
        );
    }

    /// Leftovers are the forbidden names still whole in the text, never one inside a longer name.
    #[test]
    fn leftovers_are_whole_names_only() {
        let forbidden = ["admins", "admin_sessions", "'admin'"];
        assert_eq!(
            leftovers("FROM identity.admin_sessions, admins_id_seq", &forbidden),
            ["admin_sessions"]
        );
        assert_eq!(
            leftovers("x admins_id_seq readmins", &forbidden),
            Vec::<&str>::new()
        );
        assert_eq!(leftovers("realm = 'admin'", &forbidden), ["'admin'"]);
    }
}
