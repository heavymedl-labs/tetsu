//! Tenancy's inbound HTTP adapter: a tenant's public page at its address (ADR-0007).

use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use axum::Router;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::domain::{Error, Tenant};
use crate::ports::TenantStore;
use crate::use_cases::Tenants;
use tetsu_kernel::Scheme;

/// How long finding a tenant may take before the page gives up. The pool's wait and the statement timeout
/// bound each step (ADR-0011); this bounds the request as a whole.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Template)]
#[template(path = "tenant.html")]
struct TenantPage<'a> {
    demo: bool,
    stylesheet: &'static str,
    theme: String,
    name: &'a str,
    locations: Vec<&'a str>,
}

/// A tenant's colors, loaded after the shared stylesheet so they win (ADR-0032).
#[derive(Template)]
#[template(path = "theme.css", escape = "none")]
struct ThemeSheet<'a> {
    primary: &'a str,
    accent: &'a str,
}

#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundPage {
    demo: bool,
    stylesheet: &'static str,
    /// The radio the site footer's color scheme control checks, from the visitor's cookie (ADR-0070).
    scheme: &'static str,
}

#[derive(Template)]
#[template(path = "unavailable.html")]
struct UnavailablePage {
    demo: bool,
    stylesheet: &'static str,
}

/// A theme is data that can change, so it is kept only briefly.
const THEME_CACHE: &str = "public, max-age=300";

/// Serves each tenant's public page at its address, `/<slug>`. An address no tenant holds gets the not found
/// page, never another tenant's. It sets no fallback, since axum allows one per site and the composition root
/// sets it (#90). With `demo`, every page says it shows
/// demo businesses and asks search engines not to index it (ADR-0018). A page that could not be loaded is shown
/// as unavailable, and `report` gets a line for the operator, since where such lines go is the composition
/// root's choice (#27). Each page links `stylesheet`, the shared sheet's URL, which the composition root knows,
/// and a tenant's page links its own theme sheet, `/<slug>/theme.css`, after it.
pub fn router<S>(
    tenants: Tenants<S>,
    demo: bool,
    stylesheet: &'static str,
    report: fn(&str),
) -> Router
where
    S: TenantStore + Send + Sync + 'static,
{
    routes(tenants, demo, stylesheet, RESOLVE_TIMEOUT, report)
}

/// The router, with the time a lookup may take and where failures are reported as parameters, so tests need not
/// wait out the real timeout and can read what the operator would see.
fn routes<S>(
    tenants: Tenants<S>,
    demo: bool,
    stylesheet: &'static str,
    timeout: Duration,
    report: fn(&str),
) -> Router
where
    S: TenantStore + Send + Sync + 'static,
{
    Router::new()
        .route("/{address}", get(page::<S>))
        .route("/{address}/theme.css", get(theme::<S>))
        .with_state(Arc::new(Served {
            tenants,
            demo,
            stylesheet,
            timeout,
            report,
        }))
}

struct Served<S> {
    tenants: Tenants<S>,
    demo: bool,
    stylesheet: &'static str,
    timeout: Duration,
    report: fn(&str),
}

async fn page<S: TenantStore>(
    State(served): State<Arc<Served<S>>>,
    headers: HeaderMap,
    segment: Result<Path<String>, PathRejection>,
) -> Response {
    match lookup(&served, segment, scheme_of(&headers)).await {
        Ok(tenant) => show(&tenant, &served),
        Err(response) => *response,
    }
}

async fn theme<S: TenantStore>(
    State(served): State<Arc<Served<S>>>,
    headers: HeaderMap,
    segment: Result<Path<String>, PathRejection>,
) -> Response {
    let tenant = match lookup(&served, segment, scheme_of(&headers)).await {
        Ok(tenant) => tenant,
        Err(response) => return *response,
    };
    theme_sheet(&tenant)
}

/// A tenant's theme sheet. Rendering a compiled template fails only if writing fails, as for pages.
fn theme_sheet(tenant: &Tenant) -> Response {
    let theme = tenant.theme();
    let sheet = ThemeSheet {
        primary: theme.primary.as_str(),
        accent: theme.accent.as_str(),
    };
    sheet.render().map_or_else(
        |_| StatusCode::SERVICE_UNAVAILABLE.into_response(),
        |body| {
            let headers = [
                (header::CONTENT_TYPE, "text/css; charset=utf-8"),
                (header::CACHE_CONTROL, THEME_CACHE),
            ];
            (headers, body).into_response()
        },
    )
}

/// The tenant at a path segment, or the page to show instead: not found, in the visitor's color scheme, for an
/// address no tenant holds, and unavailable when the lookup fails or takes too long.
async fn lookup<S: TenantStore>(
    served: &Served<S>,
    segment: Result<Path<String>, PathRejection>,
    scheme: Scheme,
) -> Result<Tenant, Box<Response>> {
    // A path that does not decode, such as one that is not UTF-8, is no tenant's address.
    let Ok(Path(segment)) = segment else {
        return Err(Box::new(not_found(served, scheme)));
    };
    let address = format!("/{segment}");
    let failure = match tokio::time::timeout(served.timeout, served.tenants.resolve(&address)).await
    {
        Ok(Ok(tenant)) => {
            // The composition root's request span carries the tenant into every line the request logs (ADR-0045).
            tracing::Span::current().record("tenant_id", tenant.id().get());
            return Ok(tenant);
        }
        Ok(Err(Error::NotFound)) => return Err(Box::new(not_found(served, scheme))),
        Ok(Err(error)) => error.to_string(),
        Err(_) => format!("no answer within {} seconds", served.timeout.as_secs_f32()),
    };
    // The page says only that it is unavailable, since the detail could name the database. The operator gets
    // the detail through `report`.
    (served.report)(&failure_line(&failure));
    Err(Box::new(html(
        StatusCode::SERVICE_UNAVAILABLE,
        &UnavailablePage {
            demo: served.demo,
            stylesheet: served.stylesheet,
        },
    )))
}

/// What the operator sees when a page could not be loaded: the cause, and never the address, which is whatever
/// the visitor put in the path, such as an email (ADR-0045). The log's JSON escapes control characters.
fn failure_line(failure: &str) -> String {
    format!("tenancy: could not load a page: {failure}")
}

fn show<S>(tenant: &Tenant, served: &Served<S>) -> Response {
    let page = TenantPage {
        demo: served.demo,
        stylesheet: served.stylesheet,
        theme: format!("{}/theme.css", tenant.slug().address()),
        name: tenant.name(),
        locations: tenant
            .locations()
            .iter()
            .map(crate::Location::name)
            .collect(),
    };
    html(StatusCode::OK, &page)
}

fn not_found<S>(served: &Served<S>, scheme: Scheme) -> Response {
    html(
        StatusCode::NOT_FOUND,
        &NotFoundPage {
            demo: served.demo,
            stylesheet: served.stylesheet,
            scheme: scheme.as_str(),
        },
    )
}

/// The color scheme a request's cookies keep, which the site footer on the not-found page shows (ADR-0070).
fn scheme_of(headers: &HeaderMap) -> Scheme {
    Scheme::from_cookies(
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok()),
    )
}

/// Renders a page as HTML. A template that fails to render, which a compiled template does only if writing
/// fails, gets a bare unavailable status rather than half a page.
fn html(status: StatusCode, page: &impl Template) -> Response {
    match page.render() {
        Ok(body) => (
            status,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response(),
        // A compiled template fails only if writing fails, so there is nothing for the operator to act on.
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    /// Stands in for the shared stylesheet's hashed URL, which the composition root passes.
    const STYLESHEET: &str = "/assets/tetsu-test.css";
    use crate::domain::{Color, Location, NewTenant, Slug, TenantId, Theme};

    /// Tenants kept in memory, keyed by address, with a switch that makes every lookup fail or stall.
    #[derive(Default)]
    struct Memory {
        tenants: Mutex<HashMap<String, Tenant>>,
        failure: Option<Failure>,
    }

    #[derive(Clone, Copy)]
    enum Failure {
        Storage,
        Stall,
    }

    impl TenantStore for Memory {
        fn insert(&self, tenant: &NewTenant) -> impl Future<Output = Result<Tenant, Error>> + Send {
            let mut tenants = self.tenants.lock().unwrap();
            let id = i64::try_from(tenants.len()).unwrap() + 1;
            let stored = tenant.stored_as(TenantId::new(id));
            tenants.insert(tenant.slug().address(), stored.clone());
            std::future::ready(Ok(stored))
        }

        async fn find_by_address(&self, address: &str) -> Result<Option<Tenant>, Error> {
            match self.failure {
                Some(Failure::Storage) => {
                    Err(Error::Storage("password authentication failed".into()))
                }
                Some(Failure::Stall) => std::future::pending().await,
                None => Ok(self.tenants.lock().unwrap().get(address).cloned()),
            }
        }

        fn find_addresses_in_use(
            &self,
            addresses: &[String],
        ) -> impl Future<Output = Result<Vec<String>, Error>> + Send {
            let tenants = self.tenants.lock().unwrap();
            let held = addresses.iter().filter(|a| tenants.contains_key(*a));
            std::future::ready(Ok(held.cloned().collect()))
        }

        /// Moves the tenant to the new slug's address; enough for routing, which is all these tests serve.
        fn rename(
            &self,
            address: &str,
            to: &Slug,
        ) -> impl Future<Output = Result<bool, Error>> + Send {
            let mut tenants = self.tenants.lock().unwrap();
            let moved = tenants.remove(address);
            let found = moved.is_some();
            if let Some(tenant) = moved {
                tenants.insert(to.address(), tenant);
            }
            std::future::ready(Ok(found))
        }
    }

    fn tenant(slug: &str, name: &str) -> NewTenant {
        let theme = Theme {
            primary: Color::parse("#3A4250").unwrap(),
            accent: Color::parse("#E5622B").unwrap(),
        };
        let locations = vec![
            Location::new("Downtown", "America/Denver").unwrap(),
            Location::new("Northside", "America/Denver").unwrap(),
        ];
        NewTenant::new(Slug::parse(slug).unwrap(), name, theme, locations).unwrap()
    }

    async fn serving(failure: Option<Failure>, names: &[(&str, &str)]) -> Router {
        let tenants = Tenants::new(Memory {
            failure,
            ..Memory::default()
        });
        for (slug, name) in names {
            tenants.create(&tenant(slug, name)).await.unwrap();
        }
        routes(
            tenants,
            true,
            STYLESHEET,
            Duration::from_millis(100),
            record,
        )
    }

    static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn record(line: &str) {
        REPORTED.lock().unwrap().push(line.to_string());
    }

    /// Serves the app on a port of its own and sends it one HTTP/1.1 request, as a browser would.
    async fn get(app: Router, path: &str) -> (StatusCode, Option<String>, String) {
        let (status, head, body) = fetch(app, path).await;
        (status, header_in(&head, "content-type"), body)
    }

    /// One header's value from a raw response head.
    fn header_in(head: &str, name: &str) -> Option<String> {
        head.lines()
            .filter_map(|line| line.split_once(": "))
            .find(|(found, _)| found.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.to_string())
    }

    /// Sends one request over a real socket, and returns the status, the raw head, and the body.
    async fn fetch(app: Router, path: &str) -> (StatusCode, String, String) {
        fetch_with(app, path, "").await
    }

    /// As [`fetch`], with the browser's cookies.
    async fn fetch_with(app: Router, path: &str, cookie: &str) -> (StatusCode, String, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut stream = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: test\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        let (head, body) = raw.split_once("\r\n\r\n").unwrap();
        let code = head.split(' ').nth(1).unwrap().parse().unwrap();
        (
            StatusCode::from_u16(code).unwrap(),
            head.to_string(),
            body.to_string(),
        )
    }

    /// The theme is data: its own small sheet, after the shared one, kept only briefly (ADR-0032).
    /// A renamed tenant is served at its new address and no longer at its old one, and renaming an address no
    /// tenant holds is not found (#90).
    #[tokio::test]
    async fn a_renamed_tenant_moves_to_its_new_address() {
        let tenants = Tenants::new(Memory::default());
        tenants
            .create(&tenant("sample-studio", "Sample Studio"))
            .await
            .unwrap();
        let to = Slug::parse("sample-gym").unwrap();
        tenants.rename(" /Sample-Studio ", &to).await.unwrap();
        assert_eq!(
            tenants.rename("/sample-studio", &to).await,
            Err(Error::NotFound)
        );
        assert_eq!(
            tenants.rename("/sample\u{7}", &to).await,
            Err(Error::NotFound)
        );
        let app = routes(
            tenants,
            true,
            STYLESHEET,
            Duration::from_millis(100),
            record,
        );
        assert_eq!(get(app.clone(), "/sample-gym").await.0, StatusCode::OK);
        assert_eq!(get(app, "/sample-studio").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_tenants_theme_sheet_sets_its_colors_and_is_kept_briefly() {
        let app = serving(None, &[("anvil-row", "Anvil Row Strength")]).await;
        let (status, head, sheet) = fetch(app, "/anvil-row/theme.css").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            header_in(&head, "content-type").as_deref(),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(
            header_in(&head, "cache-control").as_deref(),
            Some("public, max-age=300")
        );
        assert!(sheet.contains("--tetsu-color-primary: #3A4250;"), "{sheet}");
        assert!(sheet.contains("--tetsu-color-accent: #E5622B;"), "{sheet}");
        // The text on brand colors stays white in both schemes, until #96 chooses it for contrast.
        assert!(
            sheet.contains("--tetsu-color-on-primary: #ffffff;"),
            "{sheet}"
        );
    }

    #[tokio::test]
    async fn an_unknown_tenants_theme_sheet_is_not_found() {
        let (status, _, _) = get(serving(None, &[]).await, "/nobody/theme.css").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, _) = get(
            serving(Some(Failure::Storage), &[]).await,
            "/anvil-row/theme.css",
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_tenant_page_shows_its_name_locations_and_theme_and_no_demo_note() {
        let app = serving(None, &[("anvil-row", "Anvil Row Strength")]).await;
        let (status, kind, body) = get(app, "/anvil-row").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(kind.as_deref(), Some("text/html; charset=utf-8"));
        assert!(body.contains("<h1>Anvil Row Strength</h1>"), "{body}");
        assert!(body.contains("<li>Downtown</li>"), "{body}");
        assert!(body.contains("<li>Northside</li>"), "{body}");
        assert!(body.contains(r#"href="/assets/tetsu-test.css""#), "{body}");
        assert!(body.contains(r#"href="/anvil-row/theme.css""#), "{body}");
        assert!(!body.contains("Demo business."), "{body}");
        assert!(
            body.contains(r#"<meta name="robots" content="noindex">"#),
            "{body}"
        );
    }

    #[tokio::test]
    async fn an_address_no_business_holds_is_shown_in_the_kept_scheme() {
        for (path, cookie, checked) in [
            ("/nobody", "theme=light", "light"),
            ("/nobody/theme.css", "a=1; theme=dark", "dark"),
            ("/nobody", "", "system"),
        ] {
            let (status, _, body) = fetch_with(serving(None, &[]).await, path, cookie).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            let radio = format!("id=\"tetsu-scheme-{checked}\" class=\"sr-only\" checked");
            assert!(body.contains(&radio), "{radio} in {body}");
        }
    }

    #[tokio::test]
    async fn outside_a_demo_environment_the_page_has_no_demo_marking() {
        let tenants = Tenants::new(Memory::default());
        tenants
            .create(&tenant("anvil-row", "Anvil Row Strength"))
            .await
            .unwrap();
        let app = routes(
            tenants,
            false,
            STYLESHEET,
            Duration::from_millis(100),
            record,
        );
        let (status, _, body) = get(app, "/anvil-row").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1>Anvil Row Strength</h1>"), "{body}");
        assert!(!body.contains("Demo business."), "{body}");
        assert!(!body.contains("noindex"), "{body}");
        let tenants = Tenants::new(Memory::default());
        let (status, _, body) = get(
            routes(
                tenants,
                false,
                STYLESHEET,
                Duration::from_millis(100),
                record,
            ),
            "/nobody",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(
            !body.contains("Demo") && !body.contains("noindex"),
            "{body}"
        );
        // An address no business holds gets the site's own not-found page, Tetsu's header and footer included.
        for shown in [
            "<title>Page not found – Tetsu</title>",
            "<nav aria-label=\"Tetsu\"",
            "<svg role=\"img\" aria-label=\"404\"",
            "© 2026 Tetsu",
        ] {
            assert!(body.contains(shown), "{shown} in {body}");
        }
    }

    #[tokio::test]
    async fn in_a_demo_environment_every_page_stays_out_of_search_with_no_note() {
        let (status, _, missing) = get(serving(None, &[]).await, "/nobody").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, unavailable) =
            get(serving(Some(Failure::Storage), &[]).await, "/anvil-row").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        for body in [missing, unavailable] {
            assert!(!body.contains("<strong>Demo.</strong>"), "{body}");
            assert!(
                body.contains(r#"<meta name="robots" content="noindex">"#),
                "{body}"
            );
        }
    }

    #[test]
    fn the_operator_line_names_the_cause_and_never_the_address() {
        assert_eq!(
            failure_line("db down"),
            "tenancy: could not load a page: db down"
        );
    }

    /// Axum panics when two merged routers both have a fallback, so the site sets the only one (#90).
    #[tokio::test]
    async fn the_router_leaves_the_fallback_to_the_site() {
        let names = [("anvil-row", "Anvil Row Strength")];
        let site = Router::new()
            .fallback(|| async { (StatusCode::NOT_FOUND, "site") })
            .merge(serving(None, &names).await);
        let (status, _, body) = get(site, "/anvil-row/extra").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, "site");
    }

    #[tokio::test]
    async fn the_public_router_serves_tenant_pages() {
        let tenants = Tenants::new(Memory::default());
        tenants
            .create(&tenant("anvil-row", "Anvil Row Strength"))
            .await
            .unwrap();
        let (status, _, body) = get(router(tenants, true, STYLESHEET, record), "/anvil-row").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1>Anvil Row Strength</h1>"), "{body}");
    }

    #[tokio::test]
    async fn an_address_reaches_only_its_own_tenant() {
        let names = [
            ("anvil-row", "Anvil Row Strength"),
            ("juniper-lane", "Juniper Lane"),
        ];
        let (_, _, body) = get(serving(None, &names).await, "/juniper-lane").await;
        assert!(body.contains("Juniper Lane"), "{body}");
        assert!(!body.contains("Anvil Row"), "{body}");
    }

    #[tokio::test]
    async fn an_unknown_address_gets_the_not_found_page_and_no_tenant() {
        let names = [("anvil-row", "Anvil Row Strength")];
        for path in [
            "/nobody",
            "/anvil-row-",
            "/%20anvil-row%0A",
            "/%FF",
            "/anvil-row%C3",
            "/anvil%2Drow%00",
        ] {
            let (status, kind, body) = get(serving(None, &names).await, path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(kind.as_deref(), Some("text/html; charset=utf-8"), "{path}");
            assert!(
                body.contains("There is nothing booked at this address."),
                "{path}"
            );
            assert!(!body.contains("Anvil Row"), "{path}");
        }
    }

    #[tokio::test]
    async fn text_from_a_tenant_is_shown_as_text_never_as_html() {
        let app = serving(None, &[("lot-42", "<script>alert(1)</script> & \"Co\"")]).await;
        let (status, _, body) = get(app, "/lot-42").await;
        assert_eq!(status, StatusCode::OK);
        assert!(!body.contains("<script>alert"), "{body}");
        assert!(
            body.contains("&#60;script&#62;alert(1)&#60;/script&#62; &#38; &#34;Co&#34;"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn a_failure_reaches_the_operator_with_its_detail() {
        get(
            serving(Some(Failure::Storage), &[]).await,
            "/reported-studio",
        )
        .await;
        let reported = REPORTED.lock().unwrap().clone();
        assert!(
            reported
                .iter()
                .any(|line| line.starts_with("tenancy: could not load a page: ")
                    && line.contains("password authentication failed")),
            "{reported:?}"
        );
        assert!(
            !reported.iter().any(|line| line.contains("reported-studio")),
            "{reported:?}"
        );
    }

    #[tokio::test]
    async fn a_storage_failure_says_unavailable_without_the_detail() {
        let (status, kind, body) =
            get(serving(Some(Failure::Storage), &[]).await, "/anvil-row").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(kind.as_deref(), Some("text/html; charset=utf-8"));
        assert!(body.contains("could not be loaded"), "{body}");
        assert!(!body.contains("password"), "{body}");
    }

    #[tokio::test]
    async fn a_stalled_lookup_gives_up_after_the_timeout() {
        let (status, _, body) = get(serving(Some(Failure::Stall), &[]).await, "/anvil-row").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("could not be loaded"), "{body}");
    }
}
