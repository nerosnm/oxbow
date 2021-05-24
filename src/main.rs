use clap::Clap;
use eyre::Result;
use opts::Opts;
use oxbow::{settings::Settings, Bot};
use surf::Client as SurfClient;
use thiserror::Error;
use twitch_api2::TwitchClient;

mod opts;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let settings = get_settings()?;

    Bot::new(settings)?.run().await?;

    let _client = TwitchClient::<'_, SurfClient>::new();

    Ok(())
}

/// Try to get the settings from various sources:
///
/// - The config file, either at the path specified by the `--config`
/// argument, or the default location `./oxbow.toml`.
/// - Command line arguments
/// - Environment variables
fn get_settings() -> Result<Settings, GetSettingsError> {
    let mut c = config::Config::new();

    dotenv::dotenv()?;

    let opts = Opts::parse();

    let settings = c.try_into()?;
    Ok(settings)
}

#[derive(Debug, Error)]
pub enum GetSettingsError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    DotEnv(#[from] dotenv::Error),
}
