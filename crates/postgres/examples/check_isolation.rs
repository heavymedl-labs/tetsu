//! Hosted test 8 (ADR-0042): checks the server version, runs the catalog checks over every module schema in the
//! database at `DATABASE_URL`, and confirms each module's role sees no tenant-owned row while no tenant is set.
//! Prints each problem. Run it as `tetsu_owner`, as docs/operations/demo-environment.md shows.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match check().await {
        Ok(problems) if problems.is_empty() => {
            println!("no isolation problems");
            std::process::ExitCode::SUCCESS
        }
        Ok(problems) => {
            for problem in problems {
                println!("{problem}");
            }
            std::process::ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn check() -> Result<Vec<String>, tetsu_postgres::Error> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set")?;
    let settings = tetsu_postgres::Settings::new(&url);
    tetsu_postgres::require_supported_server(&settings).await?;
    let client = tetsu_postgres::connect(&settings).await?;
    let schemas = tetsu_postgres::module_schemas(&client).await?;
    let schemas: Vec<&str> = schemas.iter().map(String::as_str).collect();
    let mut problems = tetsu_postgres::isolation_violations(&client, &schemas).await?;
    problems.extend(tetsu_postgres::rows_seen_without_a_tenant(&client, &schemas).await?);
    Ok(problems)
}
