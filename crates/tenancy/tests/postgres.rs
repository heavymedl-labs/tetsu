//! Tenancy against a real Postgres.

use tetsu_postgres::test_support::{TestDatabase, fresh_database};
use tetsu_postgres::{migrate, pool};
use tetsu_tenancy::{
    Color, Error, Location, NewTenant, Slug, Tenancy, TenantStore, Theme, create_tenant,
    postgres_store, resolve,
};

#[cfg(test)]
mod support {
    use super::*;

    /// A migrated store, with the test database it uses. Keep both until the test ends: dropping the database
    /// drops everything in it.
    pub async fn store_and_settings() -> (impl TenantStore, TestDatabase) {
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
            "Anvil Row Strength",
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
        "anvil-row",
        &[
            ("Denver, LoDo", "America/Denver"),
            ("Scottsdale", "America/Phoenix"),
        ],
    );
    let created = create_tenant(&store, &new).await.expect("creates");
    let found = resolve(&store, "/anvil-row").await.expect("resolves");
    assert_eq!(found, created);
    assert_eq!(found.name(), "Anvil Row Strength");
    assert_eq!(found.theme().accent.as_str(), "#E5622B");
    let names: Vec<_> = found.locations().iter().map(Location::name).collect();
    assert_eq!(names, ["Denver, LoDo", "Scottsdale"]);
}

#[tokio::test]
async fn an_address_is_matched_without_case_or_spaces() {
    let (store, _database) = store_and_settings().await;
    create_tenant(&store, &tenant("anvil-row", &[]))
        .await
        .expect("creates");
    assert!(resolve(&store, "  /Anvil-Row ").await.is_ok());
}

#[tokio::test]
async fn an_unknown_address_fails_closed() {
    let (store, _database) = store_and_settings().await;
    create_tenant(&store, &tenant("anvil-row", &[]))
        .await
        .expect("creates");
    for address in [
        "",
        "anvil-row",
        "/juniper-lane",
        "/anvil",
        "/anvil-row-x",
        "/anvil-row/",
        "//anvil-row",
    ] {
        assert_eq!(
            resolve(&store, address).await,
            Err(Error::NotFound),
            "{address:?}"
        );
    }
}

#[tokio::test]
async fn a_slug_is_taken_once_and_the_loser_leaves_nothing_behind() {
    let (store, _database) = store_and_settings().await;
    let first = create_tenant(
        &store,
        &tenant("anvil-row", &[("Denver", "America/Denver")]),
    )
    .await
    .expect("first");
    let second = create_tenant(&store, &tenant("anvil-row", &[("Elsewhere", "UTC")])).await;
    assert_eq!(second, Err(Error::SlugTaken));
    assert_eq!(
        resolve(&store, "/anvil-row").await.expect("resolves"),
        first
    );
}

#[tokio::test]
async fn tenants_see_only_their_own_locations() {
    let (store, _database) = store_and_settings().await;
    create_tenant(
        &store,
        &tenant("anvil-row", &[("Denver", "America/Denver")]),
    )
    .await
    .expect("first");
    create_tenant(
        &store,
        &tenant("juniper-lane", &[("Tucson", "America/Phoenix")]),
    )
    .await
    .expect("second");
    let juniper = resolve(&store, "/juniper-lane").await.expect("resolves");
    assert_eq!(
        juniper.locations(),
        [Location::new("Tucson", "America/Phoenix").expect("location")]
    );
}

#[tokio::test]
async fn a_lost_database_is_temporarily_unavailable() {
    let settings =
        tetsu_postgres::Settings::from_values("postgres://x@127.0.0.1:1/x", Some("disable"));
    let store = postgres_store(pool(&settings).expect("pool"));
    assert!(matches!(
        resolve(&store, "/anvil-row").await,
        Err(Error::Unavailable(_))
    ));
    assert!(matches!(
        create_tenant(&store, &tenant("anvil-row", &[])).await,
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
    let Err(Error::Storage(why)) = create_tenant(&store, &tenant("anvil-row", &[])).await else {
        panic!("a refused commit is a storage error, not worth retrying");
    };
    assert_eq!(why, "commit refused: refused at commit");
}

#[tokio::test]
async fn a_lost_database_says_why() {
    let settings =
        tetsu_postgres::Settings::from_values("postgres://x@127.0.0.1:1/x", Some("disable"));
    let store = postgres_store(pool(&settings).expect("pool"));
    let Err(Error::Unavailable(why)) = resolve(&store, "/anvil-row").await else {
        panic!("a refused connection is temporary");
    };
    assert!(why.contains("Connection refused"), "{why}");
}

#[tokio::test]
async fn a_refused_insert_that_is_not_a_taken_slug_is_a_storage_error() {
    let (store, settings) = store_and_settings().await;
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("ALTER TABLE tenancy.tenants ADD CONSTRAINT refuse CHECK (name <> 'Anvil Row Strength')")
        .await
        .expect("adds a check");
    assert!(matches!(
        create_tenant(&store, &tenant("anvil-row", &[])).await,
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
    let refused = create_tenant(&store, &tenant("anvil-row", &[("Denver", "Refused")])).await;
    assert!(matches!(refused, Err(Error::Storage(_))));
    assert_eq!(resolve(&store, "/anvil-row").await, Err(Error::NotFound));
    assert!(
        create_tenant(&store, &tenant("anvil-row", &[]))
            .await
            .is_ok(),
        "the slug is still free"
    );
}

#[tokio::test]
async fn a_stale_cached_query_is_temporary_and_clears_the_cache() {
    let (store, settings) = store_and_settings().await;
    create_tenant(&store, &tenant("anvil-row", &[]))
        .await
        .expect("creates");
    resolve(&store, "/anvil-row")
        .await
        .expect("caches the query");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("ALTER TABLE tenancy.tenants ALTER COLUMN name TYPE varchar(200)")
        .await
        .expect("alters");
    assert!(matches!(
        resolve(&store, "/anvil-row").await,
        Err(Error::Unavailable(_))
    ));
    assert!(
        resolve(&store, "/anvil-row").await.is_ok(),
        "the next request succeeds"
    );
}

#[tokio::test]
async fn a_bad_stored_row_is_an_error_not_a_bad_page() {
    let (store, settings) = store_and_settings().await;
    create_tenant(&store, &tenant("anvil-row", &[]))
        .await
        .expect("creates");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("UPDATE tenancy.tenants SET primary_color = 'red'")
        .await
        .expect("corrupts the row");
    assert!(matches!(
        resolve(&store, "/anvil-row").await,
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
        resolve(&store, "/anvil-row").await,
        Err(Error::Storage(_))
    ));
}
