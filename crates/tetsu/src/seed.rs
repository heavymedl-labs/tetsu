//! Demo business files (ADR-0018). These types are the file format's contract. They hold plain values and
//! convert into Tenancy's checked types, so the format and the domain can change separately.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tetsu_tenancy::{Color, Error, Location, NewTenant, Slug, TenantStore, Theme, create_tenant};

/// A path as printed, with every character [`escaped`] if it needs to be, so a file name cannot split a line,
/// drive the terminal, or reorder the text around it.
pub fn shown(path: &Path) -> String {
    path.display().to_string().chars().map(escaped).collect()
}

/// A character as printed. Anything Rust would not print as is, such as a control character, a bidi control,
/// a zero-width character, a separator, or a combining mark, becomes an escape like `\u{202e}`. Quotes and
/// backslashes stay. Each character is judged alone, so the result does not depend on the text around it, or
/// on how a standard library version treats combining marks within a string.
pub fn escaped(c: char) -> String {
    if matches!(c, '"' | '\'' | '\\') {
        return c.to_string();
    }
    c.escape_debug().collect()
}

/// The newest format this binary reads.
const FORMAT: i64 = 1;

/// One checked file, ready to apply.
pub struct Seed {
    pub path: PathBuf,
    pub tenant: NewTenant,
}

/// Read before the rest, so a newer file is reported as newer rather than as unknown fields.
#[derive(Deserialize)]
struct Version {
    format: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    /// Already checked through [`Version`].
    #[serde(rename = "format")]
    _format: i64,
    tenant: TenantFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TenantFile {
    slug: String,
    name: String,
    theme: ThemeFile,
    #[serde(default)]
    locations: Vec<LocationFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    primary: String,
    accent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocationFile {
    name: String,
    time_zone: String,
}

impl TenantFile {
    /// Converts into Tenancy's checked values. A location's error says which location, counting from 1.
    fn checked(&self) -> Result<NewTenant, String> {
        let theme = Theme {
            primary: Color::parse(&self.theme.primary).map_err(|error| error.to_string())?,
            accent: Color::parse(&self.theme.accent).map_err(|error| error.to_string())?,
        };
        let locations = self
            .locations
            .iter()
            .enumerate()
            .map(|(index, location)| {
                Location::new(&location.name, &location.time_zone)
                    .map_err(|error| format!("location {}: {error}", index + 1))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let slug = Slug::parse(&self.slug).map_err(|error| error.to_string())?;
        NewTenant::new(slug, &self.name, theme, locations).map_err(|error| error.to_string())
    }
}

/// Parses one file's text. Errors from TOML carry the line and column.
fn parse(text: &str) -> Result<NewTenant, String> {
    let version: Version = toml::from_str(text).map_err(|error| error.to_string())?;
    let format = version.format;
    if format > FORMAT {
        return Err(format!(
            "format {format} is newer than this tetsu reads, which is {FORMAT}"
        ));
    }
    if format != FORMAT {
        return Err(format!("format {format} is not one tetsu has used"));
    }
    let file: File = toml::from_str(text).map_err(|error| error.to_string())?;
    file.tenant.checked()
}

/// Reads and checks a file, or every `.toml` file at the top of a folder in name order, leaving out hidden
/// files. Every problem is reported, and nothing is returned unless every file passes.
pub fn read(path: &Path) -> Result<Vec<Seed>, String> {
    let mut seeds: Vec<Seed> = Vec::new();
    let mut problems = Vec::new();
    let mut slugs: HashMap<String, PathBuf> = HashMap::new();
    for file in files(path)? {
        let tenant = match load(&file) {
            Ok(tenant) => tenant,
            Err(why) => {
                // TOML errors run over several lines, so indent the rest under the file they belong to.
                let why = why.trim_end().replace('\n', "\n  ");
                problems.push(format!("{}: {why}", shown(&file)));
                continue;
            }
        };
        let slug = tenant.slug().as_str().to_string();
        if let Some(first) = slugs.get(&slug) {
            let (file, first) = (shown(&file), shown(first));
            problems.push(format!("{file}: slug {slug:?} is also in {first}"));
        } else {
            slugs.insert(slug, file.clone());
            seeds.push(Seed { path: file, tenant });
        }
    }
    if problems.is_empty() {
        Ok(seeds)
    } else {
        Err(problems.join("\n"))
    }
}

/// Reads only a regular file, so a device, a pipe, or a broken link fails rather than hanging or vanishing.
fn load(file: &Path) -> Result<NewTenant, String> {
    let metadata = std::fs::metadata(file).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("not a regular file".into());
    }
    let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
    parse(&text)
}

fn files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.is_dir() {
        return Ok(vec![path.to_path_buf()]);
    }
    let failed = |error: std::io::Error| format!("{}: {error}", shown(path));
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).map_err(failed)? {
        let file = entry.map_err(failed)?.path();
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        if name.ends_with(".toml") && !name.starts_with('.') {
            files.push(file);
        }
    }
    if files.is_empty() {
        return Err(format!("{}: no .toml files", shown(path)));
    }
    files.sort();
    Ok(files)
}

/// Creates each tenant through Tenancy's use case, in order, and skips one whose slug exists, passing a line
/// for each to `report`. A storage failure stops the run: tenants already created stay, and a rerun skips them.
pub async fn apply(
    store: &impl TenantStore,
    seeds: &[Seed],
    mut report: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    for seed in seeds {
        let (slug, file) = (seed.tenant.slug().as_str(), shown(&seed.path));
        match create_tenant(store, &seed.tenant).await {
            Ok(_) => report(&format!("created {slug} from {file}"))?,
            Err(Error::SlugTaken) => report(&format!("skipped {slug}: it already exists"))?,
            Err(error) => return Err(format!("could not create {slug} from {file}: {error}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetsu_tenancy::Tenant;

    /// Refuses every write with one error, such as a lost database or a taken slug.
    struct Refuses(Error);

    impl TenantStore for Refuses {
        fn insert(&self, _: &NewTenant) -> impl Future<Output = Result<Tenant, Error>> + Send {
            std::future::ready(Err(self.0.clone()))
        }

        fn find_by_address(
            &self,
            _: &str,
        ) -> impl Future<Output = Result<Option<Tenant>, Error>> + Send {
            std::future::ready(Ok(None))
        }
    }

    fn sample_seed() -> [Seed; 1] {
        [Seed {
            path: PathBuf::from("demos/sample.toml"),
            tenant: parse(SAMPLE).unwrap(),
        }]
    }

    #[tokio::test]
    async fn a_failed_write_names_the_tenant_not_a_fault_in_the_file() {
        let error = apply(
            &Refuses(Error::Unavailable("connection closed".into())),
            &sample_seed(),
            |_| Ok(()),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            "could not create sample-studio from demos/sample.toml: temporarily unavailable: connection closed"
        );
    }

    #[tokio::test]
    async fn a_report_that_cannot_be_written_stops_the_run() {
        let failed = |_: &str| Err("could not write output".to_string());
        let error = apply(&Refuses(Error::SlugTaken), &sample_seed(), failed)
            .await
            .unwrap_err();
        assert_eq!(error, "could not write output");
    }

    const SAMPLE: &str = include_str!("../tests/fixtures/sample.toml");

    #[test]
    fn a_file_becomes_a_checked_tenant() {
        let tenant = parse(SAMPLE).unwrap();
        assert_eq!(tenant.slug().as_str(), "sample-studio");
        assert_eq!(tenant.name(), "Sample Studio");
        assert_eq!(tenant.theme().accent.as_str(), "#E5622B");
        let zones: Vec<_> = tenant.locations().iter().map(Location::time_zone).collect();
        assert_eq!(zones, ["America/Denver", "America/Phoenix"]);
    }

    #[test]
    fn locations_may_be_left_out() {
        let text = SAMPLE.split("[[tenant.locations]]").next().unwrap();
        assert_eq!(parse(text).unwrap().locations(), &[]);
    }

    #[test]
    fn the_format_is_checked_before_anything_else() {
        let newer = SAMPLE.replace("format = 1", "format = 2\nsomething_new = true");
        assert_eq!(
            parse(&newer).unwrap_err(),
            "format 2 is newer than this tetsu reads, which is 1"
        );
        let older = SAMPLE.replace("format = 1", "format = 0");
        assert_eq!(
            parse(&older).unwrap_err(),
            "format 0 is not one tetsu has used"
        );
        let missing = SAMPLE.replace("format = 1", "");
        assert!(
            parse(&missing)
                .unwrap_err()
                .contains("missing field `format`")
        );
    }

    #[test]
    fn unknown_and_duplicate_fields_fail_with_their_line() {
        let unknown = SAMPLE.replace("accent =", "acent =");
        let error = parse(&unknown).unwrap_err();
        assert!(error.contains("line 10"), "{error}");
        assert!(error.contains("unknown field `acent`"), "{error}");

        let duplicate = SAMPLE.replace("name = \"Sample Studio\"", "name = \"A\"\nname = \"B\"");
        assert!(parse(&duplicate).unwrap_err().contains("duplicate key"));
    }

    #[test]
    fn the_domain_decides_what_a_valid_value_is() {
        let cases = [
            (
                "\"sample-studio\"",
                "\"Sample Studio\"",
                "lowercase letters",
            ),
            ("\"#3A4250\"", "\"navy\"", "must be #RRGGBB"),
            (
                "\"America/Phoenix\"",
                "\" \"",
                "location 2: invalid: a location needs",
            ),
            ("\"Sample Studio\"", "\"\"", "a name must be"),
            // Postgres cannot store a NUL, so it must fail here, before anything is written.
            (
                "\"Sample Studio\"",
                "\"Bad\\u0000Name\"",
                "control characters",
            ),
        ];
        for (from, to, expected) in cases {
            let error = parse(&SAMPLE.replacen(from, to, 1)).unwrap_err();
            assert!(error.contains(expected), "{to}: {error}");
        }
    }
}
