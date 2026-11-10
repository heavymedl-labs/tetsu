//! Tenancy's inbound HTTP adapter: a tenant's public page at its address (ADR-0007).

use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use axum::Router;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::domain::{Error, Tenant};
use crate::ports::TenantStore;
use crate::use_cases::Tenants;

/// How long finding a tenant may take before the page gives up. The pool's wait and the statement timeout
/// bound each step (ADR-0011); this bounds the request as a whole.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Template)]
#[template(path = "tenant.html")]
struct TenantPage<'a> {
    demo: bool,
    name: &'a str,
    primary: &'a str,
    accent: &'a str,
    locations: Vec<&'a str>,
}

#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundPage {
    demo: bool,
}

#[derive(Template)]
#[template(path = "unavailable.html")]
struct UnavailablePage {
    demo: bool,
}

/// Serves each tenant's public page at its address, `/<slug>`. An address no tenant holds gets the not found
/// page, never another tenant's. It sets no fallback, since axum allows one per site and the composition root
/// sets it (#90). With `demo`, every page says it shows
/// demo businesses and asks search engines not to index it (ADR-0018). A page that could not be loaded is shown
/// as unavailable, and `report` gets a line for the operator, since where such lines go is the composition
/// root's choice (#27).
pub fn router<S>(tenants: Tenants<S>, demo: bool, report: fn(&str)) -> Router
where
    S: TenantStore + Send + Sync + 'static,
{
    routes(tenants, demo, RESOLVE_TIMEOUT, report)
}

/// The router, with the time a lookup may take and where failures are reported as parameters, so tests need not
/// wait out the real timeout and can read what the operator would see.
fn routes<S>(tenants: Tenants<S>, demo: bool, timeout: Duration, report: fn(&str)) -> Router
where
    S: TenantStore + Send + Sync + 'static,
{
    Router::new()
        .route("/{address}", get(page::<S>))
        .with_state(Arc::new(Served {
            tenants,
            demo,
            timeout,
            report,
        }))
}

struct Served<S> {
    tenants: Tenants<S>,
    demo: bool,
    timeout: Duration,
    report: fn(&str),
}

async fn page<S: TenantStore>(
    State(served): State<Arc<Served<S>>>,
    segment: Result<Path<String>, PathRejection>,
) -> Response {
    // A path that does not decode, such as one that is not UTF-8, is no tenant's address.
    let Ok(Path(segment)) = segment else {
        return not_found(served.demo);
    };
    let address = format!("/{segment}");
    let failure = match tokio::time::timeout(served.timeout, served.tenants.resolve(&address)).await
    {
        Ok(Ok(tenant)) => return show(&tenant, served.demo),
        Ok(Err(Error::NotFound)) => return not_found(served.demo),
        Ok(Err(error)) => error.to_string(),
        Err(_) => format!("no answer within {} seconds", served.timeout.as_secs_f32()),
    };
    // The page says only that it is unavailable, since the detail could name the database. The operator gets
    // the detail on stderr; where logs go is #27's decision.
    (served.report)(&failure_line(&address, &failure));
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        &UnavailablePage { demo: served.demo },
    )
}

/// What the operator sees when a page could not be loaded. Both parts are escaped, so an address or a server
/// message cannot write control characters to the log.
fn failure_line(address: &str, failure: &str) -> String {
    format!(
        "tenancy: could not load {}: {}",
        address.escape_debug(),
        failure.escape_debug()
    )
}

fn show(tenant: &Tenant, demo: bool) -> Response {
    let theme = tenant.theme();
    let page = TenantPage {
        demo,
        name: tenant.name(),
        primary: theme.primary.as_str(),
        accent: theme.accent.as_str(),
        locations: tenant
            .locations()
            .iter()
            .map(crate::Location::name)
            .collect(),
    };
    html(StatusCode::OK, &page)
}

fn not_found(demo: bool) -> Response {
    html(StatusCode::NOT_FOUND, &NotFoundPage { demo })
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
        routes(tenants, true, Duration::from_millis(100), record)
    }

    static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn record(line: &str) {
        REPORTED.lock().unwrap().push(line.to_string());
    }

    /// Serves the app on a port of its own and sends it one HTTP/1.1 request, as a browser would.
    async fn get(app: Router, path: &str) -> (StatusCode, Option<String>, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut stream = TcpStream::connect(address).await.unwrap();
        let request = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        let (head, body) = raw.split_once("\r\n\r\n").unwrap();
        let mut lines = head.lines();
        let code = lines
            .next()
            .unwrap()
            .split(' ')
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let kind = lines
            .filter_map(|line| line.split_once(": "))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.to_string());
        (StatusCode::from_u16(code).unwrap(), kind, body.to_string())
    }

    #[tokio::test]
    async fn a_tenant_page_shows_its_name_locations_theme_and_that_it_is_a_demo() {
        let app = serving(None, &[("anvil-row", "Anvil Row Strength")]).await;
        let (status, kind, body) = get(app, "/anvil-row").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(kind.as_deref(), Some("text/html; charset=utf-8"));
        assert!(body.contains("<h1>Anvil Row Strength</h1>"), "{body}");
        assert!(body.contains("<li>Downtown</li>"), "{body}");
        assert!(body.contains("<li>Northside</li>"), "{body}");
        assert!(body.contains("--primary: #3A4250"), "{body}");
        assert!(body.contains("--accent: #E5622B"), "{body}");
        assert!(body.contains("Demo business."), "{body}");
        assert!(
            body.contains(r#"<meta name="robots" content="noindex">"#),
            "{body}"
        );
    }

    #[tokio::test]
    async fn outside_a_demo_environment_the_page_has_no_demo_marking() {
        let tenants = Tenants::new(Memory::default());
        tenants
            .create(&tenant("anvil-row", "Anvil Row Strength"))
            .await
            .unwrap();
        let app = routes(tenants, false, Duration::from_millis(100), record);
        let (status, _, body) = get(app, "/anvil-row").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1>Anvil Row Strength</h1>"), "{body}");
        assert!(!body.contains("Demo business."), "{body}");
        assert!(!body.contains("noindex"), "{body}");
        let tenants = Tenants::new(Memory::default());
        let (status, _, body) = get(
            routes(tenants, false, Duration::from_millis(100), record),
            "/nobody",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(
            !body.contains("Demo") && !body.contains("noindex"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn in_a_demo_environment_every_page_says_so() {
        let (status, _, missing) = get(serving(None, &[]).await, "/nobody").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, unavailable) =
            get(serving(Some(Failure::Storage), &[]).await, "/anvil-row").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        for body in [missing, unavailable] {
            assert!(body.contains("<strong>Demo.</strong>"), "{body}");
            assert!(
                body.contains(r#"<meta name="robots" content="noindex">"#),
                "{body}"
            );
        }
    }

    #[test]
    fn the_operator_line_names_the_address_and_cause_with_control_characters_escaped() {
        assert_eq!(
            failure_line("/anvil-row\n", "db \u{1b}[31mdown"),
            "tenancy: could not load /anvil-row\\n: db \\u{1b}[31mdown"
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
        let (status, _, body) = get(router(tenants, true, record), "/anvil-row").await;
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
            assert!(body.contains("No business uses this address."), "{path}");
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
            reported.iter().any(|line| line
                .starts_with("tenancy: could not load /reported-studio: ")
                && line.contains("password authentication failed")),
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
