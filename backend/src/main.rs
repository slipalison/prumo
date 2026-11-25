use std::process::ExitCode;

use prumo::config::Config;

#[tokio::main]
async fn main() -> ExitCode {
    prumo::telemetry::init_logging();
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(error = %error, "configuracao invalida");
            return ExitCode::FAILURE;
        }
    };
    match prumo::run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %error, "o prumo parou");
            ExitCode::FAILURE
        }
    }
}
