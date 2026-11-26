//! The composition root (ADR-0003).

mod assets;
mod logs;
// build.rs embeds the front-end build with it. Compiled here too, its tests and mutants cover it.
mod admin;
mod chores;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only build.rs and the tests call it")
)]
mod manifest;
mod operator;
mod seed;
mod serve;
mod settings;
mod staff;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use tetsu_audit::Audit;
use tetsu_identity::Identity;
use tetsu_kernel::Module;
use tetsu_mail::Mail;
use tetsu_tenancy::Tenancy;

/// Every module the binary runs, in the order their migrations apply.
const MODULES: &[&dyn Module] = &[&Tenancy, &Identity, &Audit, &Mail];
const USAGE: &str = "usage: tetsu [--env <name>] [setup | migrate | seed [--check] <file or folder> | serve | check | reset <schema>... \
                     | replay <schema> <event id> | rename <address> <new slug> \
                     | admin <create <name> <email> | invite <id> | list | disable <id> | enable <id> \
                     | remove-passkeys <id> | end-sessions <id | --all>> \
                     | staff invite <business> <name> <email> --role owner]";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Summary,
    Setup,
    Migrate,
    Seed { path: PathBuf, check_only: bool },
    Serve,
    Chore(chores::Chore),
    Admin(admin::Admin),
    Staff(staff::Staff),
}

fn parse(args: &[String]) -> Result<Command, String> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(Command::Summary);
    };
    match command.as_str() {
        "setup" => bare(rest, Command::Setup, "setup takes no arguments"),
        "migrate" => bare(rest, Command::Migrate, "migrate takes no arguments"),
        "serve" => bare(
            rest,
            Command::Serve,
            "serve takes no arguments; set PORT to choose the port",
        ),
        "seed" => parse_seed(rest),
        "admin" => admin::parse(rest).map(Command::Admin),
        "staff" => staff::parse(rest).map(Command::Staff),
        other => chores::parse(other, rest).map_or_else(
            || Err(format!("unknown command {command:?}")),
            |chore| chore.map(Command::Chore),
        ),
    }
}

/// A command that takes no arguments, or the reason it was refused.
fn bare(rest: &[String], command: Command, why: &str) -> Result<Command, String> {
    if rest.is_empty() {
        Ok(command)
    } else {
        Err(why.into())
    }
}

fn parse_seed(rest: &[String]) -> Result<Command, String> {
    match rest {
        [flag, path] if flag == "--check" && !path.is_empty() && !path.starts_with('-') => {
            Ok(Command::Seed {
                path: PathBuf::from(path),
                check_only: true,
            })
        }
        [path] if !path.is_empty() && !path.starts_with('-') => Ok(Command::Seed {
            path: PathBuf::from(path),
            check_only: false,
        }),
        _ => Err("seed takes one file or folder, after --check to only check it".into()),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let command = std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "arguments must be UTF-8".to_string())
        .and_then(|args| {
            let (source, rest) = settings::Source::chosen(&args)?;
            parse(rest).map(|command| (source, command))
        });
    let (source, command) = match command {
        Ok(chosen) => chosen,
        Err(why) => {
            complain(&format!("{why}\n{USAGE}"));
            return ExitCode::from(2);
        }
    };
    match run(command, &source).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            complain(&why);
            ExitCode::FAILURE
        }
    }
}

/// Runs a command with the settings `source` names.
async fn run(command: Command, source: &settings::Source) -> Result<(), String> {
    match command {
        Command::Summary => say(&summary(MODULES)),
        Command::Setup => run_setup(source).await,
        Command::Migrate => run_migrate(source).await,
        Command::Seed { path, check_only } => run_seed(&path, check_only, source).await,
        Command::Serve => run_serve(source).await,
        Command::Chore(chore) => run_chore(chore, source).await,
        Command::Admin(command) => run_admin(command, source).await,
        Command::Staff(command) => run_staff(command, source).await,
    }
}

/// Runs an admin command with the local settings, telling it whether its output reaches a terminal.
async fn run_admin(command: admin::Admin, source: &settings::Source) -> Result<(), String> {
    let terminal = std::io::IsTerminal::is_terminal(&std::io::stdout());
    say_all(&admin::run(command, || local(source), terminal).await?)
}

/// Runs a staff command with the local settings, telling it whether its output reaches a terminal.
async fn run_staff(command: staff::Staff, source: &settings::Source) -> Result<(), String> {
    let terminal = std::io::IsTerminal::is_terminal(&std::io::stdout());
    say_all(&staff::run(command, || local(source), terminal).await?)
}

/// Runs a hosted chore with the local settings, and prints what it reports.
async fn run_chore(chore: chores::Chore, source: &settings::Source) -> Result<(), String> {
    say_all(&chores::run(chore, &local(source)?, MODULES).await?)
}

/// Creates the database the settings name if the server has none, then migrates: a developer's one step to a
/// database the server can run on, safe to repeat, since it never drops or changes one that exists.
async fn run_setup(source: &settings::Source) -> Result<(), String> {
    let local = local(source)?;
    let settings = settings::database(|name| local.var(name))?;
    let created = tetsu_postgres::create_database(&settings)
        .await
        .map_err(|error| explain(&*error))?;
    say(if created {
        "created the database"
    } else {
        "the database exists"
    })?;
    run_migrate(source).await
}

/// Runs each module's new migrations.
async fn run_migrate(source: &settings::Source) -> Result<(), String> {
    let local = local(source)?;
    let settings = settings::database(|name| local.var(name))?;
    tetsu_postgres::migrate(&settings, MODULES)
        .await
        .map_err(|error| explain(&*error))?;
    say(&format!("migrated: {}", names(MODULES)))
}

/// Refuses to serve while a tenant holds an address a reserved name now claims, since a site route would hide
/// that tenant without a word (#90).
async fn refuse_reserved_addresses<S: tetsu_tenancy::TenantStore>(
    tenants: &tetsu_tenancy::Tenants<S>,
) -> Result<(), String> {
    let held = tenants
        .reserved_addresses_in_use()
        .await
        .map_err(|error| format!("could not check tenants' addresses: {error}"))?;
    if held.is_empty() {
        return Ok(());
    }
    Err(format!(
        "tenants hold addresses reserved for Tetsu's own pages: {}; give each a new address before serving",
        held.join(", ")
    ))
}

/// Serves the public site until asked to stop. Settings are checked and the database reached before it listens,
/// so a bad setting fails at start rather than on the first request. A failure or panic while starting is logged
/// as an alert, and the logs flushed, before the process exits. The alert's line says only that serving could not
/// start, since the reason can name a tenant's address or quote the database; the reason goes to stderr.
async fn run_serve(source: &settings::Source) -> Result<(), String> {
    let stop = serve::stop_signal();
    let local = local(source)?;
    let terminal = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let format = settings::log_format(|name| local.var(name), terminal)?;
    let logs = logs::start(settings::export(|name| local.var(name)), format)?;
    let started = tokio::spawn(async move { ready(&local).await })
        .await
        .unwrap_or_else(|_| Err("serving panicked while starting".into()));
    let served = match started {
        Ok((listener, app, dispatcher)) => {
            let deliver = async move |stopped| dispatcher.run(DELIVERY_POLL, stopped).await;
            serve_and_deliver(listener, app, deliver, stop).await
        }
        Err(why) => {
            tracing::error!(alert = "start_failed", "could not start serving");
            Err(why)
        }
    };
    if served.is_ok() {
        tracing::info!("stopped");
    }
    logs.stop();
    served
}

/// How often the dispatcher looks for events another process recorded, or ones due to be tried again; one this
/// process records wakes it at once (ADR-0043).
const DELIVERY_POLL: std::time::Duration = std::time::Duration::from_secs(10);

/// Serves and delivers events side by side until `stop` finishes. Both then wind down together within the same
/// grace, after which delivery is cancelled; an unfinished pass rolls back, and its events go on the next start.
/// Should delivery stop on its own, which only a panic does, serving stops too, so the host starts the process again
/// rather than serve with no events delivered.
async fn serve_and_deliver<F>(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    deliver: impl FnOnce(Stopped) -> F,
    stop: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), String>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let (stopping, stopped) = tokio::sync::watch::channel(false);
    let mut delivering = tokio::spawn(deliver(until(stopped.clone())));
    let serving = async {
        let served = serve::run(listener, app, until(stopped.clone()), serve::GRACE).await;
        stopping.send_replace(true);
        served
    };
    let watching = async {
        // Delivery ends only once told to stop, so ending first means it stopped on its own. A server that failed
        // tells it to stop too, so that failure also raises this alert beside its own error.
        let died = tokio::select! {
            () = stop => false,
            _ = &mut delivering => true,
        };
        stopping.send_replace(true);
        if died {
            tracing::error!(
                alert = "delivery_stopped",
                "events stopped being delivered, so serving stops for a restart"
            );
        } else {
            let _ = tokio::time::timeout(serve::GRACE, &mut delivering).await;
        }
        // Past the grace, delivery is cancelled; a task already finished is left as it is.
        delivering.abort();
        died
    };
    let (served, died) = tokio::join!(serving, watching);
    served.map_err(|error| format!("serving stopped: {error}"))?;
    if died {
        return Err("event delivery stopped".to_string());
    }
    Ok(())
}

/// Finishes once serving is told to stop.
type Stopped = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

fn until(mut stopped: tokio::sync::watch::Receiver<bool>) -> Stopped {
    Box::pin(async move {
        let _ = stopped.wait_for(|stopping| *stopping).await;
    })
}

/// Reads the settings, reaches the database, checks it is migrated, and binds the port: everything serving needs
/// before it listens, with the dispatcher that delivers events to the audit record and the admins' notices.
async fn ready(
    local: &settings::Local,
) -> Result<
    (
        tokio::net::TcpListener,
        axum::Router,
        tetsu_postgres::Dispatcher,
    ),
    String,
> {
    let demo = settings::demo(|name| local.var(name))?;
    let proxies = settings::proxies(|name| local.var(name))?;
    let address = settings::listen(|name| local.var(name))?;
    let (smtp, allow) = mail(local)?;
    let pool = migrated(local).await?;
    let office = std::sync::Arc::new(tetsu_mail::post_office(pool.clone(), smtp, allow));
    let dispatcher = delivery(&pool, &office).await?;
    refuse_reserved_addresses(&tetsu_tenancy::postgres_store(pool.clone())).await?;
    let (modules, home) = served(local, demo, &office, pool)?;
    let site = serve::Site {
        demo,
        proxies,
        home,
    };
    let app = serve::app(modules, site, serve::not_found);
    Ok((bind(address).await?, app, dispatcher))
}

/// The routers of what `TETSU_SERVES` names, and where the root sends a visitor on a host that serves a realm but
/// not the site. Each realm is served only on its own host (ADR-0040, #169); this machine serves every one.
fn served(
    local: &settings::Local,
    demo: bool,
    office: &std::sync::Arc<tetsu_mail::PostOffice>,
    pool: tetsu_postgres::Pool,
) -> Result<(axum::Router, Option<&'static str>), String> {
    let serves = settings::serves(|name| local.var(name))?;
    let mut modules = axum::Router::new();
    let names = Businesses(tetsu_tenancy::postgres_store(pool.clone()));
    let businesses: std::sync::Arc<dyn tetsu_identity::BusinessNames> = std::sync::Arc::new(names);
    let pages = |origin| realm_pages(local, demo, office, &businesses, origin);
    if serves.site {
        let tenants = tetsu_tenancy::postgres_store(pool.clone());
        modules = modules.merge(tetsu_tenancy::router(
            tenants,
            demo,
            assets::STYLESHEET,
            report,
        ));
    }
    if serves.admin {
        let admins = tetsu_identity::postgres_store(pool.clone());
        modules = modules.merge(tetsu_identity::router(
            admins,
            pages(settings::ADMIN_ORIGIN)?,
        ));
    }
    if serves.staff {
        let staff = tetsu_identity::postgres_staff_store(pool);
        modules = modules.merge(tetsu_identity::staff_router(
            staff,
            pages(settings::STAFF_ORIGIN)?,
        ));
    }
    let home = match serves {
        settings::Serves { site: true, .. } => None,
        settings::Serves { admin: true, .. } => Some("/admin"),
        _ => Some("/app"),
    };
    Ok((modules, home))
}

/// A pool to the database once no module's database is behind its code (#164).
async fn migrated(local: &settings::Local) -> Result<tetsu_postgres::Pool, String> {
    let pool = connect(local).await?;
    let storages: Vec<_> = MODULES
        .iter()
        .filter_map(|module| module.storage())
        .collect();
    match tetsu_postgres::behind_code(&pool, &storages).await {
        Ok(None) => Ok(pool),
        Ok(Some(behind)) => Err(refusal(behind)),
        Err(error) => Err(explain(&*error)),
    }
}

/// Why serving cannot start on a database behind its code, and what fixes it.
fn refusal(behind: tetsu_postgres::Behind) -> String {
    match behind {
        tetsu_postgres::Behind::Unapplied { .. } => format!(
            "{behind}. Apply new migrations with `tetsu migrate`, or `cargo tetsu setup` locally"
        ),
        tetsu_postgres::Behind::Changed { schema, .. } => format!(
            "{behind}. An applied migration must stay as it was; locally, `cargo tetsu reset {schema}` rebuilds its \
             schema"
        ),
    }
}

/// Binds `address`, and says where serving listens.
async fn bind(address: std::net::SocketAddr) -> Result<tokio::net::TcpListener, String> {
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not listen on {address}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("could not read the listening address: {error}"))?;
    tracing::info!("serving on http://{bound}");
    Ok(listener)
}

/// The SMTP adapter and allowlist the mail settings name, which a stray `.env` may set only beside the database
/// (ADR-0074).
pub(crate) fn mail(
    local: &settings::Local,
) -> Result<(Option<tetsu_mail::Smtp>, tetsu_mail::Allowlist), String> {
    local.beside("DATABASE_URL", &settings::MAIL)?;
    local.together(&settings::MAIL)?;
    let mail = settings::mail(|name| local.var(name))?;
    Ok((settings::smtp(&mail)?, mail.allow))
}

/// The dispatcher that delivers every module's events to the audit record, and Identity's to the notices, which
/// `office` sends.
async fn delivery(
    pool: &tetsu_postgres::Pool,
    office: &std::sync::Arc<tetsu_mail::PostOffice>,
) -> Result<tetsu_postgres::Dispatcher, String> {
    let schemas: Vec<&str> = MODULES
        .iter()
        .filter_map(|module| module.storage())
        .map(|storage| storage.schema)
        .collect();
    let consumers = vec![
        // Every module that records events today; one that starts to must join this list, or its events reach no
        // audit record (ADR-0045).
        tetsu_audit::consumer(&["identity"]),
        tetsu_identity::notices(office.clone()),
    ];
    tetsu_postgres::Dispatcher::new(pool.clone(), &schemas, consumers)
        .await
        .map_err(|error| explain(&*error))
}

/// What a realm's pages need: their passkeys belong to the realm's own origin, from the setting `name`, the only one
/// that may send them a state-changing request.
fn realm_pages(
    local: &settings::Local,
    demo: bool,
    office: &std::sync::Arc<tetsu_mail::PostOffice>,
    businesses: &std::sync::Arc<dyn tetsu_identity::BusinessNames>,
    name: &str,
) -> Result<tetsu_identity::Web, String> {
    local.beside("DATABASE_URL", &[name])?;
    let origin = settings::origin(|name| local.var(name), name)?;
    Ok(tetsu_identity::Web {
        party: tetsu_identity::RelyingParty::new(&origin).map_err(|error| error.to_string())?,
        origins: tetsu_identity::Origins::parse(&[&origin]).map_err(|error| error.to_string())?,
        demo,
        stylesheet: assets::STYLESHEET,
        script: assets::PASSKEY_SCRIPT,
        session_script: assets::SESSION_SCRIPT,
        scheme_script: assets::SCHEME_SCRIPT,
        menu_script: assets::MENU_SCRIPT,
        mailbox: office.clone(),
        businesses: businesses.clone(),
        report,
    })
}

/// Businesses' names, read from Tenancy for Identity's staff invite page (#111).
struct Businesses<S>(tetsu_tenancy::Tenants<S>);

impl<S: tetsu_tenancy::TenantStore + Send + Sync> tetsu_identity::BusinessNames for Businesses<S> {
    fn name(&self, business: tetsu_identity::BusinessId) -> tetsu_identity::Naming<'_> {
        Box::pin(async move {
            let named = self.0.name(business.get()).await;
            named.map_err(|error| tetsu_identity::Error::Storage(error.to_string()))
        })
    }
}

/// Logs what a module reports, as an error on the request it happened in.
fn report(line: &str) {
    tracing::error!("{line}");
}

/// Checks every file before anything is written, and writes only through Tenancy's use cases (ADR-0018).
async fn run_seed(
    path: &std::path::Path,
    check_only: bool,
    source: &settings::Source,
) -> Result<(), String> {
    let seeds = seed::read(path)?;
    if check_only {
        for checked in &seeds {
            let slug = checked.tenant.slug().as_str();
            say(&format!(
                "checked {slug} from {}",
                seed::shown(&checked.path)
            ))?;
        }
        return Ok(());
    }
    let pool = connect(&local(source)?).await?;
    seed::apply(&tetsu_tenancy::postgres_store(pool), &seeds, say).await
}

/// The settings in the file `source` names where the command runs, for any the environment leaves unset (ADR-0050,
/// ADR-0073).
fn local(source: &settings::Source) -> Result<settings::Local, String> {
    settings::Local::load(std::path::Path::new("."), source)
}

/// Reads the database settings, checks the server is new enough and stores text as UTF-8, and opens a pool to it.
async fn connect(local: &settings::Local) -> Result<tetsu_postgres::Pool, String> {
    local.beside("DATABASE_URL", &["DATABASE_TLS", "DATABASE_CA_FILE"])?;
    let settings = settings::database(|name| local.var(name))?;
    tetsu_postgres::require_supported_server(&settings)
        .await
        .map_err(|error| explain(&*error))?;
    tetsu_postgres::pool(&settings).map_err(|error| explain(&*error))
}

/// Writes a line to stdout. A closed pipe or a full disk is an error rather than a panic.
fn say(line: &str) -> Result<(), String> {
    writeln!(std::io::stdout(), "{}", printable(line))
        .map_err(|error| format!("could not write output: {error}"))
}

/// Writes each line to stdout.
fn say_all(lines: &[String]) -> Result<(), String> {
    lines.iter().try_for_each(|line| say(line))
}

/// Writes an error to stderr. If even that fails there is nowhere left to report it, so the exit code carries it.
fn complain(why: &str) {
    let _ = writeln!(std::io::stderr(), "error: {}", printable(why));
}

/// Escapes what could drive the terminal, reorder text, or hide in it, other than the line breaks errors use,
/// so text quoted from a file or a server cannot rewrite the lines around it.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c == '\n' {
                c.to_string()
            } else {
                seed::escaped(c)
            }
        })
        .collect()
}

/// An error and each cause under it. The driver's own text is often only "error connecting to server".
fn explain(error: &(dyn std::error::Error + 'static)) -> String {
    let causes: Vec<String> = std::iter::successors(Some(error), |error| error.source())
        .map(ToString::to_string)
        .collect();
    causes.join(": ")
}

/// Words as a command line holds them, for tests.
#[cfg(test)]
fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(ToString::to_string).collect()
}

pub(crate) fn names(modules: &[&dyn Module]) -> String {
    if modules.is_empty() {
        return "none".to_string();
    }
    let names: Vec<&str> = modules.iter().map(|module| module.name()).collect();
    names.join(", ")
}

fn summary(modules: &[&dyn Module]) -> String {
    format!(
        "tetsu {}, modules: {}",
        env!("CARGO_PKG_VERSION"),
        names(modules)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A service mounts what `TETSU_SERVES` names, and a host without the site sends its root to its realm's home;
    /// building the routers reaches no database.
    #[tokio::test]
    async fn a_service_mounts_what_it_serves() {
        let pool = tetsu_postgres::pool(&tetsu_postgres::Settings::from_values(
            "postgres://nobody@127.0.0.1:1/none",
            Some("disable"),
        ))
        .unwrap();
        let office = std::sync::Arc::new(tetsu_mail::post_office(
            pool.clone(),
            None,
            tetsu_mail::Allowlist::default(),
        ));
        let origins = "DATABASE_URL=postgres://nobody@127.0.0.1:1/none\nTETSU_ADMIN_ORIGIN=https://admin.example.com\nTETSU_STAFF_ORIGIN=https://app.example.com\n";
        for (serves, home) in [
            ("site,admin,staff", None),
            ("site", None),
            ("admin", Some("/admin")),
            ("admin,staff", Some("/admin")),
            ("staff", Some("/app")),
        ] {
            let local = settings::Local::file_only(&format!("{origins}TETSU_SERVES={serves}\n"));
            let (_, found) = served(&local, true, &office, pool.clone()).unwrap();
            assert_eq!(found, home, "{serves}");
        }
        let missing = settings::Local::file_only(
            "DATABASE_URL=postgres://nobody@127.0.0.1:1/none\nTETSU_SERVES=staff\n",
        );
        assert_eq!(
            served(&missing, true, &office, pool).map(|(_, home)| home),
            Err("TETSU_STAFF_ORIGIN must be set".into())
        );
    }

    /// Each way a database is behind its code gets the fix for it.
    #[test]
    fn a_database_behind_its_code_is_told_what_fixes_it() {
        let unapplied = tetsu_postgres::Behind::Unapplied {
            schema: "audit",
            version: 3,
        };
        assert_eq!(
            refusal(unapplied),
            "audit version 3 is not applied. Apply new migrations with `tetsu migrate`, or `cargo tetsu setup` locally"
        );
        let changed = tetsu_postgres::Behind::Changed {
            schema: "identity",
            version: 8,
        };
        assert_eq!(
            refusal(changed),
            "identity version 8 changed after it was applied. An applied migration must stay as it was; locally, \
             `cargo tetsu reset identity` rebuilds its schema"
        );
    }

    async fn listener() -> tokio::net::TcpListener {
        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap()
    }

    /// Delivery that ends untold stops serving with an error, so the host restarts the process.
    #[tokio::test]
    async fn serving_stops_when_delivery_stops_on_its_own() {
        let served = serve_and_deliver(
            listener().await,
            axum::Router::new(),
            async |_| {},
            std::future::pending(),
        )
        .await;
        assert_eq!(served, Err("event delivery stopped".to_string()));
    }

    /// A stop ends serving and delivery cleanly: delivery finishes its pass before serving returns, and finishing
    /// once told is no failure.
    #[tokio::test]
    async fn a_stop_waits_for_delivery_to_finish_its_pass() {
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = finished.clone();
        let served = serve_and_deliver(
            listener().await,
            axum::Router::new(),
            async move |stopped| {
                stopped.await;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            },
            async {},
        )
        .await;
        assert_eq!(served, Ok(()));
        assert!(finished.load(std::sync::atomic::Ordering::SeqCst));
    }

    struct Named(&'static str);

    impl Module for Named {
        fn name(&self) -> &'static str {
            self.0
        }
    }

    #[test]
    fn summary_says_when_no_modules_are_registered() {
        assert_eq!(
            summary(&[]),
            format!("tetsu {}, modules: none", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn summary_names_each_registered_module() {
        let modules: [&dyn Module; 2] = [&Named("tenancy"), &Named("booking")];
        assert_eq!(
            summary(&modules),
            format!(
                "tetsu {}, modules: tenancy, booking",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn output_escapes_what_could_drive_reorder_or_hide_text() {
        assert_eq!(
            printable("a\u{1b}[31mb\rc\u{202E}d\u{200B}e\u{2028}f\tg"),
            "a\\u{1b}[31mb\\rc\\u{202e}d\\u{200b}e\\u{2028}f\\tg"
        );
        assert_eq!(
            printable("line one\nCafé Cafe\u{301} \"quoted\" C:\\path 'x'"),
            "line one\nCafé Cafe\\u{301} \"quoted\" C:\\path 'x'"
        );
    }

    #[test]
    fn commands_are_parsed_exactly() {
        assert_eq!(parse(&args(&[])), Ok(Command::Summary));
        assert_eq!(parse(&args(&["setup"])), Ok(Command::Setup));
        assert_eq!(
            parse(&args(&["setup", "now"])),
            Err("setup takes no arguments".into())
        );
        assert_eq!(parse(&args(&["migrate"])), Ok(Command::Migrate));
        assert_eq!(
            parse(&args(&["migrate", "now"])),
            Err("migrate takes no arguments".into())
        );
        assert_eq!(
            parse(&args(&["Migrate"])),
            Err("unknown command \"Migrate\"".into())
        );
    }

    #[test]
    fn serve_takes_no_arguments() {
        assert_eq!(parse(&args(&["serve"])), Ok(Command::Serve));
        assert_eq!(
            parse(&args(&["serve", "8080"])),
            Err("serve takes no arguments; set PORT to choose the port".into())
        );
    }

    #[test]
    fn seed_takes_one_path_and_an_optional_check() {
        let seed = |path: &str, check_only| {
            Ok(Command::Seed {
                path: PathBuf::from(path),
                check_only,
            })
        };
        assert_eq!(parse(&args(&["seed", "demos"])), seed("demos", false));
        assert_eq!(
            parse(&args(&["seed", "--check", "demos"])),
            seed("demos", true)
        );
        let wrong = Err("seed takes one file or folder, after --check to only check it".into());
        for bad in [
            &["seed"][..],
            &["seed", "--check"],
            &["seed", "--force", "demos"],
            &["seed", "a", "b"],
            &["seed", "--chekc"],
            &["seed", "--check", "--check"],
            &["seed", "--check", "-x.toml"],
            &["seed", ""],
            &["seed", "--check", ""],
        ] {
            assert_eq!(parse(&args(bad)), wrong, "{bad:?}");
        }
    }
}
