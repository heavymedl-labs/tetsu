//! Tenancy against a real Postgres.

use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{migrate, pool};
use tetsu_tenancy::{
    Color, Error, Location, NewTenant, Slug, Tenancy, TenantStore, Tenants, Theme, postgres_store,
};

#[cfg(test)]
mod support {
    use super::*;

    /// A migrated store, with the test database it uses. Keep both until the test ends: dropping the database
    /// drops everything in it.
    pub async fn store_and_settings() -> (Tenants<impl TenantStore>, TestDatabase) {
        let settings = fresh_database().await.expect("a test database");
        migrate(&settings, &[&Tenancy]).await.expect("migrates");
        (postgres_store(pool(&settings).expect("pool")), settings)
    }

    pub fn tenant(slug: &str, locations: &[(&str, &str)]) -> NewTenant {
        let theme = Theme {
            primary: Color::parse("#3A4250").expect("color"),
            accent: Color::parse("#E5622B").expect("color"),
        };
        let locations = locations
            .iter()
            .map(|(name, zone)| Location::new(name, zone).expect("location"))
            .collect();
        NewTenant::new(
            Slug::parse(slug).expect("slug"),
            "Sample Studio",
            theme,
            locations,
        )
        .expect("tenant")
    }
}

use support::{store_and_settings, tenant};

#[tokio::test]
async fn a_created_tenant_resolves_by_its_slug() {
    let (store, _database) = store_and_settings().await;
    let new = tenant(
        "sample-studio",
        &[
            ("Main Street", "America/Denver"),
            ("Second Street", "America/Phoenix"),
        ],
    );
    let created = store.create(&new).await.expect("creates");
    let found = store.resolve("/sample-studio").await.expect("resolves");
    assert_eq!(found, created);
    assert_eq!(found.name(), "Sample Studio");
    assert_eq!(found.theme().accent.as_str(), "#E5622B");
    let names: Vec<_> = found.locations().iter().map(Location::name).collect();
    assert_eq!(names, ["Main Street", "Second Street"]);
}

#[tokio::test]
async fn an_address_is_matched_without_case_or_spaces() {
    let (store, _database) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    assert!(store.resolve("  /Sample-Studio ").await.is_ok());
}

#[tokio::test]
async fn an_unknown_address_fails_closed() {
    let (store, _database) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    for address in [
        "",
        "sample-studio",
        "/another-studio",
        "/sample",
        "/sample-studio-x",
        "/sample-studio/",
        "//sample-studio",
    ] {
        assert_eq!(
            store.resolve(address).await,
            Err(Error::NotFound),
            "{address:?}"
        );
    }
}

#[tokio::test]
async fn a_slug_is_taken_once_and_the_loser_leaves_nothing_behind() {
    let (store, _database) = store_and_settings().await;
    let first = store
        .create(&tenant("sample-studio", &[("Denver", "America/Denver")]))
        .await
        .expect("first");
    let second = store
        .create(&tenant("sample-studio", &[("Elsewhere", "UTC")]))
        .await;
    assert_eq!(second, Err(Error::SlugTaken));
    assert_eq!(
        store.resolve("/sample-studio").await.expect("resolves"),
        first
    );
}

#[tokio::test]
async fn tenants_see_only_their_own_locations() {
    let (store, _database) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[("Denver", "America/Denver")]))
        .await
        .expect("first");
    store
        .create(&tenant(
            "another-studio",
            &[("Other Street", "America/Phoenix")],
        ))
        .await
        .expect("second");
    let another = store.resolve("/another-studio").await.expect("resolves");
    assert_eq!(
        another.locations(),
        [Location::new("Other Street", "America/Phoenix").expect("location")]
    );
}

#[tokio::test]
async fn a_lost_database_is_temporarily_unavailable() {
    let settings =
        tetsu_postgres::Settings::from_values("postgres://x@127.0.0.1:1/x", Some("disable"));
    let store = postgres_store(pool(&settings).expect("pool"));
    assert!(matches!(
        store.resolve("/sample-studio").await,
        Err(Error::Unavailable(_))
    ));
    assert!(matches!(
        store.create(&tenant("sample-studio", &[])).await,
        Err(Error::Unavailable(_))
    ));
}

#[tokio::test]
async fn a_commit_refused_by_postgres_keeps_what_postgres_said() {
    let (store, settings) = store_and_settings().await;
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute(
            "CREATE FUNCTION tenancy.refuse() RETURNS trigger LANGUAGE plpgsql AS
                 $$ BEGIN RAISE EXCEPTION 'refused at commit'; END $$;
             CREATE CONSTRAINT TRIGGER refuse AFTER INSERT ON tenancy.tenants
                 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION tenancy.refuse()",
        )
        .await
        .expect("adds a trigger that fires at commit");
    let Err(Error::Storage(why)) = store.create(&tenant("sample-studio", &[])).await else {
        panic!("a refused commit is a storage error, not worth retrying");
    };
    assert_eq!(why, "commit refused: refused at commit");
}

#[tokio::test]
async fn a_lost_database_says_why() {
    let settings =
        tetsu_postgres::Settings::from_values("postgres://x@127.0.0.1:1/x", Some("disable"));
    let store = postgres_store(pool(&settings).expect("pool"));
    let Err(Error::Unavailable(why)) = store.resolve("/sample-studio").await else {
        panic!("a refused connection is temporary");
    };
    assert!(why.contains("Connection refused"), "{why}");
}

#[tokio::test]
async fn a_refused_insert_that_is_not_a_taken_slug_is_a_storage_error() {
    let (store, settings) = store_and_settings().await;
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute(
            "ALTER TABLE tenancy.tenants ADD CONSTRAINT refuse CHECK (name <> 'Sample Studio')",
        )
        .await
        .expect("adds a check");
    assert!(matches!(
        store.create(&tenant("sample-studio", &[])).await,
        Err(Error::Storage(_))
    ));
}

#[test]
fn the_module_registers_as_tenancy_with_its_own_schema() {
    use tetsu_kernel::Module;
    assert_eq!(Tenancy.name(), "tenancy");
    assert_eq!(Tenancy.storage().expect("stores data").schema, "tenancy");
}

#[tokio::test]
async fn a_failure_after_the_tenant_row_leaves_nothing_behind() {
    let (store, settings) = store_and_settings().await;
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute(
            "ALTER TABLE tenancy.locations ADD CONSTRAINT refuse CHECK (time_zone <> 'Refused')",
        )
        .await
        .expect("adds a check");
    let refused = store
        .create(&tenant("sample-studio", &[("Denver", "Refused")]))
        .await;
    assert!(matches!(refused, Err(Error::Storage(_))));
    assert_eq!(store.resolve("/sample-studio").await, Err(Error::NotFound));
    assert!(
        store.create(&tenant("sample-studio", &[])).await.is_ok(),
        "the slug is still free"
    );
}

/// A stale statement is cached on every connection that ran the query, so the first failure clears them all.
#[tokio::test]
async fn a_stale_cached_query_is_temporary_and_clears_every_connection() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Tenancy]).await.expect("migrates");
    let pool = pool(&settings).expect("pool");
    let store = postgres_store(pool.clone());
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    let find = include_str!("../sql/find_tenant_by_address.sql");
    let (first, second) = (
        pool.get().await.expect("a connection"),
        pool.get().await.expect("another"),
    );
    for connection in [&first, &second] {
        connection
            .prepare_cached(find)
            .await
            .expect("caches the query");
    }
    drop((first, second));
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("ALTER TABLE tenancy.tenants ALTER COLUMN name TYPE varchar(200)")
        .await
        .expect("alters");
    assert!(matches!(
        store.resolve("/sample-studio").await,
        Err(Error::Unavailable(_))
    ));
    let (first, second) = (
        pool.get().await.expect("a connection"),
        pool.get().await.expect("another"),
    );
    assert_eq!(
        (first.statement_cache.size(), second.statement_cache.size()),
        (0, 0),
        "every connection's cache is cleared"
    );
    drop((first, second));
    assert!(
        store.resolve("/sample-studio").await.is_ok(),
        "the next request succeeds"
    );
}

/// The insert returns the tenant's id, so changing that column's type makes its cached statement stale.
#[tokio::test]
async fn a_stale_cached_insert_is_temporary_and_clears_the_cache() {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Tenancy]).await.expect("migrates");
    let pool = pool(&settings).expect("pool");
    let store = postgres_store(pool.clone());
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("caches the insert");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("ALTER TABLE tenancy.tenants ALTER COLUMN id TYPE int")
        .await
        .expect("alters");
    assert!(matches!(
        store.create(&tenant("another-studio", &[])).await,
        Err(Error::Unavailable(_))
    ));
    let connection = pool.get().await.expect("a connection");
    assert_eq!(connection.statement_cache.size(), 0, "the cache is cleared");
}

#[tokio::test]
async fn a_bad_stored_row_is_an_error_not_a_bad_page() {
    let (store, settings) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("UPDATE tenancy.tenants SET primary_color = 'red'")
        .await
        .expect("corrupts the row");
    assert!(matches!(
        store.resolve("/sample-studio").await,
        Err(Error::Invalid(_))
    ));
}

#[tokio::test]
async fn a_login_refused_by_the_server_is_not_worth_retrying() {
    let (_, settings) = store_and_settings().await;
    let (base, database) = settings.url().rsplit_once('/').expect("names a database");
    let (scheme, host) = base.split_once("://").expect("has a scheme");
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let stranger = tetsu_postgres::Settings::from_values(
        &format!("{scheme}://nobody@{host}/{database}"),
        Some("disable"),
    );
    let store = postgres_store(pool(&stranger).expect("pool"));
    assert!(matches!(
        store.resolve("/sample-studio").await,
        Err(Error::Storage(_))
    ));
}

/// Too many connections reaches the store through the pool, as a login refusal Postgres sends with code 53300.
/// Superusers skip a database's connection limit, so a role with none to spare stands in.
#[tokio::test]
async fn a_server_out_of_connections_is_temporary_through_the_pool() {
    let (_store, settings) = store_and_settings().await;
    let role = format!("t_full_{}", std::process::id());
    let admin = tetsu_postgres::connect(&settings).await.expect("connects");
    admin
        .batch_execute(&format!("CREATE ROLE {role} LOGIN CONNECTION LIMIT 0"))
        .await
        .expect("creates the role");
    let url = settings.url().replacen("postgres@", &format!("{role}@"), 1);
    let full = postgres_store(
        pool(&tetsu_postgres::Settings::from_values(
            &url,
            Some("disable"),
        ))
        .expect("pool"),
    );
    let result = full.resolve("/sample-studio").await;
    admin
        .batch_execute(&format!("DROP ROLE {role}"))
        .await
        .expect("drops the role");
    assert!(matches!(result, Err(Error::Unavailable(_))), "{result:?}");
}

#[tokio::test]
async fn an_address_padded_with_other_whitespace_is_not_found() {
    let (store, _database) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    for address in [
        "/sample-studio\u{2028}",
        "\u{3000}/sample-studio",
        "/sample-studio\u{a0}",
    ] {
        assert_eq!(
            store.resolve(address).await,
            Err(Error::NotFound),
            "{address:?}"
        );
    }
}

#[tokio::test]
async fn an_address_with_a_control_character_is_not_found() {
    let (store, _database) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    for address in [
        "/sample-studio\0",
        "/\0",
        "/sample\u{7}studio",
        "/sample-studio\u{9b}",
    ] {
        assert_eq!(
            store.resolve(address).await,
            Err(Error::NotFound),
            "{address:?}"
        );
    }
}

/// ADR-0009: every query file prepares against the migrated schema, whether or not another test runs it.
#[tokio::test]
async fn every_query_file_prepares() {
    let (_store, settings) = store_and_settings().await;
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sql");
    let mut prepared = 0;
    for entry in std::fs::read_dir(folder).expect("lists sql/") {
        let path = entry.expect("an entry").path();
        let sql = std::fs::read_to_string(&path).expect("reads");
        client
            .prepare(&sql)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        prepared += 1;
    }
    assert_eq!(prepared, 5);
}
