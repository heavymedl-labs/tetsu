//! Drops every test database on `TEST_DATABASE_URL`. `scripts/check.sh` runs it when it finishes, so a run
//! leaves nothing behind even if a test was killed or the cleanup code was mutated (#43).

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match tetsu_postgres::test_support::drop_all_test_databases().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: could not drop test databases: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
