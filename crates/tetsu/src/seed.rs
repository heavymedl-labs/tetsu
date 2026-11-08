//! Demo business files (ADR-0018). These types are the file format's contract. They hold plain values and
//! convert into Tenancy's checked types, so the format and the domain can change separately.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tetsu_tenancy::{Color, Error, Location, NewTenant, Slug, TenantStore, Theme, create_tenant};

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
    fn checked(&self) -> Result<NewTenant, Error> {
        let theme = Theme {
            primary: Color::parse(&self.theme.primary)?,
            accent: Color::parse(&self.theme.accent)?,
        };
        let locations = self
            .locations
            .iter()
            .map(|location| Location::new(&location.name, &location.time_zone))
            .collect::<Result<Vec<_>, _>>()?;
        NewTenant::new(Slug::parse(&self.slug)?, &self.name, theme, locations)
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
    file.tenant.checked().map_err(|error| error.to_string())
}

/// Reads and checks a file, or every `.toml` file at the top of a folder in name order. Every problem is
/// reported, and nothing is returned unless every file passes.
pub fn read(path: &Path) -> Result<Vec<Seed>, String> {
    let mut seeds: Vec<Seed> = Vec::new();
    let mut problems = Vec::new();
    let mut slugs: HashMap<String, PathBuf> = HashMap::new();
    for file in files(path)? {
        let tenant = match load(&file) {
            Ok(tenant) => tenant,
            Err(why) => {
                problems.push(format!("{}: {why}", file.display()));
                continue;
            }
        };
        let slug = tenant.slug().as_str().to_string();
        if let Some(first) = slugs.get(&slug) {
            let (file, first) = (file.display(), first.display());
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

fn load(file: &Path) -> Result<NewTenant, String> {
    let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
    parse(&text)
}

fn files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.is_dir() {
        return Ok(vec![path.to_path_buf()]);
    }
    let failed = |error: std::io::Error| format!("{}: {error}", path.display());
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).map_err(failed)? {
        let file = entry.map_err(failed)?.path();
        if file.is_file()
            && file
                .extension()
                .is_some_and(|extension| extension == "toml")
        {
            files.push(file);
        }
    }
    if files.is_empty() {
        return Err(format!("{}: no .toml files", path.display()));
    }
    files.sort();
    Ok(files)
}

/// Creates each tenant through Tenancy's use case, in order, and skips one whose slug exists. A storage
/// failure stops the run: tenants already created stay, and a rerun skips them.
pub async fn apply(store: &impl TenantStore, seeds: &[Seed]) -> Result<(), String> {
    for seed in seeds {
        let slug = seed.tenant.slug().as_str();
        match create_tenant(store, &seed.tenant).await {
            Ok(_) => println!("created {slug} from {}", seed.path.display()),
            Err(Error::SlugTaken) => println!("skipped {slug}: it already exists"),
            Err(error) => return Err(format!("{}: {error}", seed.path.display())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ("\"America/Phoenix\"", "\" \"", "a time zone"),
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
