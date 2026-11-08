//! `tetsu seed` and `tetsu seed --check`, run as the real binary.

use std::path::PathBuf;

use tetsu_postgres::test_support::fresh_database;
use tetsu_tenancy::{Error, Location, resolve};

#[cfg(test)]
mod support {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU32, Ordering};

    use tetsu_postgres::{Settings, pool};
    use tetsu_tenancy::{TenantStore, postgres_store};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    pub fn sample() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.toml")
    }

    /// An empty folder of its own under Cargo's scratch space.
    pub fn folder() -> PathBuf {
        let name = format!(
            "seed-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::create_dir_all(&path).expect("a scratch folder");
        path
    }

    /// The sample file with one line replaced, written into `folder`.
    pub fn variant(folder: &Path, name: &str, from: &str, to: &str) -> PathBuf {
        let text = std::fs::read_to_string(sample()).expect("the sample");
        assert!(text.contains(from), "the sample has {from:?}");
        let path = folder.join(name);
        std::fs::write(&path, text.replace(from, to)).expect("writes");
        path
    }

    pub fn check(path: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .args(["seed", "--check"])
            .arg(path)
            .env_remove("DATABASE_URL")
            .output()
            .expect("the binary runs")
    }

    pub fn seed(settings: &Settings, path: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .arg("seed")
            .arg(path)
            .env("DATABASE_URL", settings.url())
            .env("DATABASE_TLS", "disable")
            .output()
            .expect("the binary runs")
    }

    pub fn migrate(settings: &Settings) {
        let output = Command::new(env!("CARGO_BIN_EXE_tetsu"))
            .arg("migrate")
            .env("DATABASE_URL", settings.url())
            .env("DATABASE_TLS", "disable")
            .output()
            .expect("the binary runs");
        assert!(output.status.success(), "{output:?}");
    }

    pub fn store(settings: &Settings) -> impl TenantStore {
        postgres_store(pool(settings).expect("a pool"))
    }

    pub fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("output is UTF-8")
    }
}

use support::{check, folder, migrate, sample, seed, store, text, variant};

#[test]
fn check_accepts_a_valid_file_without_a_database() {
    let output = check(&sample());
    assert!(output.status.success(), "{output:?}");
    assert!(text(&output.stdout).contains("sample-studio"));
}

#[test]
fn check_names_the_file_and_line_of_an_unknown_field() {
    let folder = folder();
    let bad = variant(&folder, "typo.toml", "accent =", "acent =");
    let output = check(&bad);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("typo.toml"), "{stderr}");
    assert!(stderr.contains("line 10"), "{stderr}");
    assert!(stderr.contains("unknown field `acent`"), "{stderr}");
}

#[test]
fn check_reports_a_newer_format_as_newer() {
    let folder = folder();
    let newer = variant(
        &folder,
        "newer.toml",
        "format = 1",
        "format = 2\nextra = true",
    );
    let stderr = text(&check(&newer).stderr);
    assert!(
        stderr.contains("format 2 is newer than this tetsu reads, which is 1"),
        "{stderr}"
    );
}

#[test]
fn check_applies_the_domain_rules() {
    let folder = folder();
    let bad = variant(
        &folder,
        "slug.toml",
        "\"sample-studio\"",
        "\"Sample Studio\"",
    );
    let stderr = text(&check(&bad).stderr);
    assert!(stderr.contains("slug.toml"), "{stderr}");
    assert!(stderr.contains("lowercase letters"), "{stderr}");
}

#[test]
fn check_reports_every_bad_file_in_a_folder() {
    let folder = folder();
    variant(&folder, "a-good.toml", "", "");
    variant(&folder, "b-typo.toml", "accent =", "acent =");
    variant(&folder, "c-color.toml", "#E5622B", "orange");
    std::fs::write(folder.join("notes.txt"), "not a seed file").expect("writes");
    let output = check(&folder);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(!stderr.contains("a-good.toml"), "{stderr}");
    assert!(stderr.contains("b-typo.toml"), "{stderr}");
    assert!(stderr.contains("c-color.toml"), "{stderr}");
    assert!(!stderr.contains("notes.txt"), "{stderr}");
}

#[test]
fn check_refuses_two_files_with_one_slug() {
    let folder = folder();
    variant(&folder, "first.toml", "", "");
    variant(&folder, "second.toml", "Sample Studio", "Another Studio");
    let stderr = text(&check(&folder).stderr);
    assert!(stderr.contains("second.toml"), "{stderr}");
    assert!(
        stderr.contains("slug \"sample-studio\" is also in"),
        "{stderr}"
    );
}

#[test]
fn check_refuses_an_empty_folder_and_a_missing_path() {
    let empty = check(&folder());
    assert_eq!(empty.status.code(), Some(1));
    assert!(text(&empty.stderr).contains("no .toml files"));

    let missing = check(&PathBuf::from("/no/such/seed.toml"));
    assert_eq!(missing.status.code(), Some(1));
    assert!(text(&missing.stderr).contains("/no/such/seed.toml"));
}

#[tokio::test]
async fn seed_creates_the_tenant_and_a_rerun_skips_it() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings);

    let first = seed(&settings, &sample());
    assert!(first.status.success(), "{first:?}");
    assert!(text(&first.stdout).contains("created sample-studio"));

    let tenant = resolve(&store(&settings), "/sample-studio")
        .await
        .expect("resolves");
    assert_eq!(tenant.name(), "Sample Studio");
    assert_eq!(tenant.theme().primary.as_str(), "#3A4250");
    let names: Vec<_> = tenant.locations().iter().map(Location::name).collect();
    assert_eq!(names, ["Main Street", "Riverside"]);

    let second = seed(&settings, &sample());
    assert!(second.status.success(), "{second:?}");
    assert!(text(&second.stdout).contains("skipped sample-studio"));
}

#[tokio::test]
async fn seed_writes_nothing_when_any_file_is_invalid() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings);
    let folder = folder();
    variant(&folder, "a-good.toml", "", "");
    variant(&folder, "b-bad.toml", "\"sample-studio\"", "\"Bad Slug\"");

    let output = seed(&settings, &folder);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        resolve(&store(&settings), "/sample-studio").await,
        Err(Error::NotFound)
    );
}

#[tokio::test]
async fn seed_before_migrate_fails_and_says_why() {
    let settings = fresh_database().await.expect("a test database");
    let output = seed(&settings, &sample());
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("tenancy.tenants"),
        "{output:?}"
    );
}
