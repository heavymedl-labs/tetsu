//! Tenancy against a real Postgres, each test on a copy of a database migrated once.

use tetsu_postgres::test_support::{TestDatabase, fresh_database, migrated_database};
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
        let settings = migrated_database(&[&Tenancy])
            .await
            .expect("a migrated test database");
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
    let settings = migrated_database(&[&Tenancy])
        .await
        .expect("a migrated test database");
    let pool = pool(&settings).expect("pool");
    let store = postgres_store(pool.clone());
    store
        .create(&tenant(
            "sample-studio",
            &[("Main Street", "America/Denver")],
        ))
        .await
        .expect("creates");
    // Tenants are found through a function, whose callers no column change reaches, so locations stand in.
    let find = include_str!("../sql/find_locations.sql");
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
        .batch_execute("ALTER TABLE tenancy.locations ALTER COLUMN name TYPE varchar(200)")
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

/// Registering returns the tenant's id, so a migration that replaces the function with another return type makes
/// its cached statement stale.
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
        .batch_execute(
            "DROP FUNCTION tenancy.register_tenant(text, text, text, text, text);
             CREATE FUNCTION tenancy.register_tenant(text, text, text, text, text) RETURNS int
               LANGUAGE sql AS 'SELECT 1'",
        )
        .await
        .expect("replaces the function");
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
    // The database refuses such a row (ADR-0060), so the check is dropped to stand for one stored before it.
    client
        .batch_execute(
            "ALTER TABLE tenancy.tenants DROP CONSTRAINT tenants_primary_color;
             UPDATE tenancy.tenants SET primary_color = 'red'",
        )
        .await
        .expect("corrupts the row");
    assert!(matches!(
        store.resolve("/sample-studio").await,
        Err(Error::Invalid(_))
    ));
}

/// The database holds tenants to the shapes Tenancy's domain writes, whatever writes them (ADR-0060).
#[tokio::test]
async fn the_database_refuses_what_the_domain_refuses() {
    let (store, settings) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[("Denver", "America/Denver")]))
        .await
        .expect("creates");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    for (statement, constraint) in [
        (
            "UPDATE tenancy.tenants SET primary_color = 'red'",
            "tenants_primary_color",
        ),
        (
            "UPDATE tenancy.tenants SET accent_color = '#e5622b'",
            "tenants_accent_color",
        ),
        ("UPDATE tenancy.tenants SET slug = 'Sample'", "tenants_slug"),
        (
            "UPDATE tenancy.tenants SET slug = repeat('a', 64)",
            "tenants_slug",
        ),
        ("UPDATE tenancy.tenants SET name = ''", "tenants_name"),
        (
            "UPDATE tenancy.tenants SET name = repeat('é', 101)",
            "tenants_name",
        ),
        (
            "UPDATE tenancy.addresses SET address = '/'",
            "addresses_address",
        ),
        (
            "UPDATE tenancy.addresses SET address = repeat('a', 254)",
            "addresses_address",
        ),
    ] {
        let refused = client.batch_execute(statement).await.expect_err(statement);
        assert_eq!(
            refused.as_db_error().and_then(|db| db.constraint()),
            Some(constraint),
            "{statement}"
        );
    }
    client
        .batch_execute("UPDATE tenancy.tenants SET name = repeat('é', 100)")
        .await
        .expect("a name of 100 characters fits, whatever its bytes");
}

/// A site route always wins over a tenant's address, so a stored address a later release reserves would hide its
/// tenant without a word; the composition root asks for them before serving (#90).
#[tokio::test]
async fn stored_addresses_reserved_since_are_found() {
    let (store, settings) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    assert_eq!(
        store.reserved_addresses_in_use().await.expect("checks"),
        Vec::<String>::new()
    );
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("UPDATE tenancy.addresses SET address = '/login'")
        .await
        .expect("stores an address reserved since");
    assert_eq!(
        store.reserved_addresses_in_use().await.expect("checks"),
        ["/login"]
    );
}

/// A slug a later release reserves fails when its row is read, rather than showing a page.
#[tokio::test]
async fn a_stored_tenant_whose_slug_is_now_reserved_is_an_error() {
    let (store, settings) = store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[]))
        .await
        .expect("creates");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    client
        .batch_execute("UPDATE tenancy.tenants SET slug = 'assets'")
        .await
        .expect("stores a slug reserved since");
    assert!(matches!(
        store.resolve("/sample-studio").await,
        Err(Error::Invalid(message)) if message.contains("reserved")
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
    // One per use the adapter makes of the database, rename_tenant.sql the newest (#135).
    assert_eq!(prepared, 6);
}

/// Logged in as the application role, as `tetsu serve` is, so row-level security applies (ADR-0042).
#[cfg(test)]
async fn app_store_and_settings() -> (Tenants<impl TenantStore>, TestDatabase) {
    let settings = fresh_database().await.expect("a test database");
    migrate(&settings, &[&Tenancy]).await.expect("migrates");
    (
        postgres_store(pool(&settings.as_app()).expect("pool")),
        settings,
    )
}

#[tokio::test]
async fn the_app_role_creates_and_resolves_tenants() {
    let (store, _database) = app_store_and_settings().await;
    let created = store
        .create(&tenant(
            "sample-studio",
            &[("Main Street", "America/Denver")],
        ))
        .await
        .expect("creates");
    assert_eq!(store.resolve("/sample-studio").await, Ok(created));
    assert_eq!(
        store.reserved_addresses_in_use().await,
        Ok(Vec::<String>::new())
    );
}

#[tokio::test]
async fn one_tenant_s_address_never_reaches_another_s_locations() {
    let (store, _database) = app_store_and_settings().await;
    store
        .create(&tenant("anvil-row", &[("Downtown", "America/Denver")]))
        .await
        .expect("creates");
    store
        .create(&tenant("juniper-lane", &[("Old Town", "America/Phoenix")]))
        .await
        .expect("creates");
    for (address, expected) in [("/anvil-row", "Downtown"), ("/juniper-lane", "Old Town")] {
        let found = store.resolve(address).await.expect("resolves");
        let names: Vec<_> = found.locations().iter().map(Location::name).collect();
        assert_eq!(names, [expected], "{address}");
    }
}

/// Once addresses come from more than slugs, a taken address must not read as a taken slug (#8).
#[tokio::test]
async fn a_taken_address_is_not_reported_as_a_taken_slug() {
    let (store, database) = app_store_and_settings().await;
    let held = store
        .create(&tenant("anvil-row", &[]))
        .await
        .expect("creates");
    tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .execute(
            "INSERT INTO tenancy.addresses (address, tenant_id) VALUES ('/free-slug', $1)",
            &[&held.id().get()],
        )
        .await
        .expect("holds a second address");
    assert_eq!(
        store.create(&tenant("free-slug", &[])).await,
        Err(Error::AddressTaken)
    );
}

/// Tenancy's own role reads tenants and addresses only through the lookups, and never rewrites them, so no
/// business can list the others or take over another's address (ADR-0042).
#[tokio::test]
async fn the_module_role_cannot_list_or_rewrite_tenants_and_addresses() {
    let (store, database) = app_store_and_settings().await;
    store
        .create(&tenant("anvil-row", &[]))
        .await
        .expect("creates");
    let app = tetsu_postgres::pool(&database.as_app()).expect("pool");
    for statement in [
        "SELECT count(*) FROM tenancy.addresses",
        "SELECT count(*) FROM tenancy.tenants",
        "UPDATE tenancy.addresses SET tenant_id = 0",
        "UPDATE tenancy.tenants SET name = 'Taken'",
        "DELETE FROM tenancy.addresses",
        "DELETE FROM tenancy.tenants",
    ] {
        let mut client = app.get().await.expect("a connection");
        let transaction = tetsu_postgres::ModuleTransaction::begin(&mut client, "tenancy")
            .await
            .expect("begins");
        let refused = transaction
            .execute(statement, &[])
            .await
            .expect_err(statement);
        assert_eq!(
            refused.code(),
            Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE),
            "{statement}"
        );
    }
}

/// Only the slug's and the address's own constraints mean "taken"; any other refusal is a storage error.
#[tokio::test]
async fn another_unique_refusal_is_a_storage_error() {
    let (store, database) = app_store_and_settings().await;
    tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .batch_execute("CREATE UNIQUE INDEX one_name ON tenancy.tenants (name)")
        .await
        .expect("adds a rule");
    store
        .create(&tenant("anvil-row", &[]))
        .await
        .expect("creates");
    let refused = store.create(&tenant("juniper-lane", &[])).await;
    assert!(
        matches!(&refused, Err(Error::Storage(why)) if why.contains("one_name")),
        "{refused:?}"
    );
}

/// Each location has its own id, so nothing that refers to one depends on its place, and two may swap places in one
/// transaction but never share one (ADR-0060).
#[tokio::test]
async fn locations_keep_their_ids_and_swap_places_but_never_share_one() {
    let (store, settings) = store_and_settings().await;
    store
        .create(&tenant(
            "sample-studio",
            &[("Denver", "America/Denver"), ("Boulder", "America/Denver")],
        ))
        .await
        .expect("creates");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    let ids = async || -> Vec<String> {
        client
            .query(
                "SELECT id::text FROM tenancy.locations ORDER BY sort_order",
                &[],
            )
            .await
            .expect("reads")
            .iter()
            .map(|row| row.try_get(0).expect("an id"))
            .collect()
    };
    let before = ids().await;
    assert_eq!(before.len(), 2);
    assert_ne!(before[0], before[1]);
    client
        .batch_execute(
            "BEGIN;
             UPDATE tenancy.locations SET sort_order = 1 - sort_order;
             COMMIT",
        )
        .await
        .expect("two places swap in one transaction");
    assert_eq!(ids().await, [before[1].clone(), before[0].clone()]);
    let shared = client
        .batch_execute("UPDATE tenancy.locations SET sort_order = 0")
        .await
        .expect_err("two locations in one place");
    assert_eq!(
        shared.as_db_error().and_then(|db| db.constraint()),
        Some("locations_place")
    );
}

/// The largest of everything the domain accepts fits the database, so its checks never refuse what Rust takes
/// (ADR-0060), and a location's name or time zone past it is refused there too.
#[tokio::test]
async fn the_database_takes_the_largest_tenant_the_domain_takes() {
    let (store, settings) = store_and_settings().await;
    let longest = "é".repeat(100);
    let largest = NewTenant::new(
        Slug::parse(&format!("a-{}", "b".repeat(61))).expect("a 63-byte slug"),
        &longest,
        Theme {
            primary: Color::parse("#ffffff").expect("color"),
            accent: Color::parse("#000000").expect("color"),
        },
        vec![Location::new(&longest, &longest).expect("a location")],
    )
    .expect("a tenant");
    store.create(&largest).await.expect("fits");
    let client = tetsu_postgres::connect(&settings).await.expect("connects");
    for (statement, constraint) in [
        ("UPDATE tenancy.locations SET name = ''", "locations_name"),
        (
            "UPDATE tenancy.locations SET name = repeat('é', 101)",
            "locations_name",
        ),
        (
            "UPDATE tenancy.locations SET time_zone = repeat('a', 101)",
            "locations_time_zone",
        ),
    ] {
        let refused = client.batch_execute(statement).await.expect_err(statement);
        assert_eq!(
            refused.as_db_error().and_then(|db| db.constraint()),
            Some(constraint),
            "{statement}"
        );
    }
}

/// A tenant whose address a release reserved moves to a new slug and its address, as the application role, and
/// keeps everything else; a taken slug changes nothing, and an address no tenant holds is not found (#90).
#[tokio::test]
async fn a_tenant_on_a_reserved_address_is_renamed() {
    let (store, settings) = app_store_and_settings().await;
    store
        .create(&tenant("sample-studio", &[("Denver", "America/Denver")]))
        .await
        .expect("creates");
    store
        .create(&tenant("another-studio", &[]))
        .await
        .expect("creates");
    // Stands for a release reserving the slug after the tenant took it.
    tetsu_postgres::connect(&settings)
        .await
        .expect("connects")
        .batch_execute(
            "UPDATE tenancy.addresses SET address = '/login' WHERE address = '/sample-studio';
             UPDATE tenancy.tenants SET slug = 'login' WHERE slug = 'sample-studio'",
        )
        .await
        .expect("reserves");
    let taken = Slug::parse("another-studio").expect("slug");
    assert_eq!(
        store.rename("/login", &taken).await,
        Err(Error::AddressTaken)
    );
    let to = Slug::parse("sample-gym").expect("slug");
    assert_eq!(store.rename("/nobody", &to).await, Err(Error::NotFound));
    store.rename("/login", &to).await.expect("renames");
    let moved = store.resolve("/sample-gym").await.expect("resolves");
    assert_eq!(moved.slug().as_str(), "sample-gym");
    assert_eq!(moved.locations().len(), 1);
    assert_eq!(store.resolve("/login").await, Err(Error::NotFound));
    assert_eq!(
        store.reserved_addresses_in_use().await.expect("checks"),
        Vec::<String>::new()
    );
}
