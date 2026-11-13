//! The composition root (ADR-0003).

mod assets;
mod logs;
// build.rs embeds the front-end build with it. Compiled here too, its tests and mutants cover it.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only build.rs and the tests call it")
)]
mod manifest;
mod seed;
mod serve;
mod settings;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use tetsu_kernel::Module;
use tetsu_tenancy::Tenancy;

/// Every module the binary runs, in the order their migrations apply.
const MODULES: &[&dyn Module] = &[&Tenancy];
const USAGE: &str = "usage: tetsu [migrate | seed [--check] <file or folder> | serve]";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Summary,
    Migrate,
    Seed { path: PathBuf, check_only: bool },
    Serve,
}

fn parse(args: &[String]) -> Result<Command, String> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(Command::Summary);
    };
    match command.as_str() {
        "migrate" if rest.is_empty() => Ok(Command::Migrate),
        "migrate" => Err("migrate takes no arguments".into()),
        "serve" if rest.is_empty() => Ok(Command::Serve),
        "serve" => Err("serve takes no arguments; set PORT to choose the port".into()),
        "seed" => parse_seed(rest),
        _ => Err(format!("unknown command {command:?}")),
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
        Command::Summary => say(&summary(MODULES))?,
        Command::Migrate => {
            let settings = settings::database(|name| std::env::var(name))?;
            tetsu_postgres::migrate(&settings, MODULES)
                .await
                .map_err(|error| explain(&*error))?;
            say(&format!("migrated: {}", names(MODULES)))?;
        }
        Command::Seed { path, check_only } => run_seed(&path, check_only).await?,
        Command::Serve => run_serve().await?,
    }
    Ok(())
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
/// so a bad setting fails at start rather than on the first request.
async fn run_serve() -> Result<(), String> {
    let stop = serve::stop_signal();
    logs::start()?;
    let demo = settings::demo(|name| std::env::var(name))?;
    let address = settings::listen(|name| std::env::var(name))?;
    let pool = connect().await?;
    let tenants = tetsu_tenancy::postgres_store(pool);
    refuse_reserved_addresses(&tenants).await?;
    let app = serve::app(
        tetsu_tenancy::router(tenants, demo, assets::STYLESHEET, report),
        demo,
        serve::not_found,
    );
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not listen on {address}: {error}"))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("could not read the listening address: {error}"))?;
    tracing::info!("serving on http://{bound}");
    serve::run(listener, app, stop, serve::GRACE)
        .await
        .map_err(|error| format!("serving stopped: {error}"))?;
    tracing::info!("stopped");
    Ok(())
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
    let pool = connect().await?;
    seed::apply(&tetsu_tenancy::postgres_store(pool), &seeds, say).await
}

/// Reads the database settings, checks the server is new enough and stores text as UTF-8, and opens a pool to it.
async fn connect() -> Result<tetsu_postgres::Pool, String> {
    let settings = settings::database(|name| std::env::var(name))?;
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

fn names(modules: &[&dyn Module]) -> String {
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

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
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
