//! The composition root (ADR-0003).

mod assets;
mod logs;
// build.rs embeds the front-end build with it. Compiled here too, its tests and mutants cover it.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only build.rs and the tests call it")
)]
mod manifest;
mod operator;
mod ops;
mod seed;
mod serve;
mod settings;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use tetsu_audit::Audit;
use tetsu_identity::Identity;
use tetsu_kernel::Module;
use tetsu_tenancy::Tenancy;

/// Every module the binary runs, in the order their migrations apply.
const MODULES: &[&dyn Module] = &[&Tenancy, &Identity, &Audit];
const USAGE: &str = "usage: tetsu [setup | migrate | seed [--check] <file or folder> | serve | check | reset <schema>... \
                     | replay <schema> <event id> | rename <address> <new slug> \
                     | operator <create <name> | invite <id> | list | disable <id> | enable <id> \
                     | remove-passkeys <id> | end-sessions <id | --all>>]";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Summary,
    Setup,
    Migrate,
    Seed { path: PathBuf, check_only: bool },
    Serve,
    Chore(ops::Chore),
    Operator(operator::Operator),
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
        "operator" => operator::parse(rest).map(Command::Operator),
        other => ops::parse(other, rest).map_or_else(
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
        .and_then(|args| parse(&args));
    let command = match command {
        Ok(command) => command,
        Err(why) => {
            complain(&format!("{why}\n{USAGE}"));
            return ExitCode::from(2);
        }
    };
    match run(command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            complain(&why);
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Summary => say(&summary(MODULES)),
        Command::Setup => run_setup().await,
        Command::Migrate => run_migrate().await,
        Command::Seed { path, check_only } => run_seed(&path, check_only).await,
        Command::Serve => run_serve().await,
        Command::Chore(chore) => run_chore(chore).await,
        Command::Operator(command) => run_operator(command).await,
    }
}

/// Runs an operator command with the local settings, telling it whether its output reaches a terminal.
async fn run_operator(command: operator::Operator) -> Result<(), String> {
    let terminal = std::io::IsTerminal::is_terminal(&std::io::stdout());
    say_all(&operator::run(command, local, terminal).await?)
}

/// Runs a hosted chore with the local settings, and prints what it reports.
async fn run_chore(chore: ops::Chore) -> Result<(), String> {
    say_all(&ops::run(chore, &local()?, MODULES).await?)
}

/// Creates the database the settings name if the server has none, then migrates: a developer's one step to a
/// database the server can run on, safe to repeat, since it never drops or changes one that exists.
async fn run_setup() -> Result<(), String> {
    let local = local()?;
    let settings = settings::database(|name| local.var(name))?;
    let created = tetsu_postgres::create_database(&settings)
        .await
        .map_err(|error| explain(&*error))?;
    say(if created {
        "created the database"
    } else {
        "the database exists"
    })?;
    run_migrate().await
}

/// Runs each module's new migrations.
async fn run_migrate() -> Result<(), String> {
    let local = local()?;
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
async fn run_serve() -> Result<(), String> {
    let stop = serve::stop_signal();
    let local = local()?;
    let terminal = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let format = settings::log_format(|name| local.var(name), terminal)?;
    let logs = logs::start(settings::export(|name| local.var(name)), format)?;
    let started = tokio::spawn(async move { ready(&local).await })
        .await
        .unwrap_or_else(|_| Err("serving panicked while starting".into()));
    let served = match started {
        Ok((listener, app)) => serve::run(listener, app, stop, serve::GRACE)
            .await
            .map_err(|error| format!("serving stopped: {error}")),
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

/// Reads the settings, reaches the database, and binds the port: everything serving needs before it listens.
async fn ready(local: &settings::Local) -> Result<(tokio::net::TcpListener, axum::Router), String> {
    let demo = settings::demo(|name| local.var(name))?;
    let proxies = settings::proxies(|name| local.var(name))?;
    let address = settings::listen(|name| local.var(name))?;
    let pool = connect(local).await?;
    let operators = operator_pages(local, demo)?;
    let tenants = tetsu_tenancy::postgres_store(pool.clone());
    refuse_reserved_addresses(&tenants).await?;
    let modules = tetsu_tenancy::router(tenants, demo, assets::STYLESHEET, report).merge(
        tetsu_identity::router(tetsu_identity::postgres_store(pool), operators),
    );
    let app = serve::app(modules, demo, proxies, serve::not_found);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not listen on {address}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("could not read the listening address: {error}"))?;
    tracing::info!("serving on http://{bound}");
    Ok((listener, app))
}

/// What Identity's operator pages need: their passkeys belong to the site's own origin, the only one that may send
/// them a state-changing request.
fn operator_pages(local: &settings::Local, demo: bool) -> Result<tetsu_identity::Web, String> {
    local.beside("DATABASE_URL", &["TETSU_ORIGIN"])?;
    let origin = settings::origin(|name| local.var(name))?;
    Ok(tetsu_identity::Web {
        party: tetsu_identity::RelyingParty::new(&origin).map_err(|error| error.to_string())?,
        origins: tetsu_identity::Origins::parse(&[&origin]).map_err(|error| error.to_string())?,
        demo,
        stylesheet: assets::STYLESHEET,
        script: assets::PASSKEY_SCRIPT,
        session_script: assets::SESSION_SCRIPT,
        scheme_script: assets::SCHEME_SCRIPT,
        report,
    })
}

/// Logs what a module reports for the operator, as an error on the request it happened in.
fn report(line: &str) {
    tracing::error!("{line}");
}

/// Checks every file before anything is written, and writes only through Tenancy's use cases (ADR-0018).
async fn run_seed(path: &std::path::Path, check_only: bool) -> Result<(), String> {
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
    let pool = connect(&local()?).await?;
    seed::apply(&tetsu_tenancy::postgres_store(pool), &seeds, say).await
}

/// The settings in `.env` where the command runs, for any the environment leaves unset (ADR-0050).
fn local() -> Result<settings::Local, String> {
    settings::Local::load(std::path::Path::new("."))
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
