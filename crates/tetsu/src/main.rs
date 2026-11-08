//! The composition root (ADR-0003).

mod settings;

use std::process::ExitCode;

use tetsu_kernel::Module;
use tetsu_tenancy::Tenancy;

/// Every module the binary runs, in the order their migrations apply.
const MODULES: &[&dyn Module] = &[&Tenancy];
const USAGE: &str = "usage: tetsu [migrate]";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Summary,
    Migrate,
}

fn parse(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Summary),
        [command] if command == "migrate" => Ok(Command::Migrate),
        [command, ..] if command == "migrate" => Err("migrate takes no arguments".into()),
        [command, ..] => Err(format!("unknown command {command:?}")),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse(&args) {
        Ok(command) => command,
        Err(why) => {
            eprintln!("error: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("error: {why}");
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Summary => println!("{}", summary(MODULES)),
        Command::Migrate => {
            let settings = settings::database(|name| std::env::var(name).ok())?;
            tetsu_postgres::migrate(&settings, MODULES)
                .await
                .map_err(|error| explain(&*error))?;
            println!("migrated: {}", names(MODULES));
        }
    }
    Ok(())
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
}
