use std::env;

use eyre::{Result, WrapErr};
use oxbow::{auth::SQLiteTokenStore, Bot};
use rusqlite::Connection;
use surf::Client as SurfClient;
use tracing::info;
use twitch_api2::TwitchClient;
use twitch_irc::{login::RefreshingLoginCredentials, ClientConfig, TCPTransport, TwitchIRCClient};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenv::dotenv().ok();

    let client_id = get_env("CLIENT_ID")?;
    let client_secret = get_env("CLIENT_SECRET")?;
    let twitch_name = get_env("TWITCH_NAME")?;
    let twitch_channel = get_env("TWITCH_CHANNEL")?;

    let mut bot = Bot::the_builder()
        .client_id(&client_id)
        .client_secret(&client_secret)
        .twitch_name(&twitch_name)
        .add_channel(&twitch_channel)
        .db_path("./oxbow.sqlite3")
        .build()?;

    bot.run().await?;

    let _client = TwitchClient::<'_, SurfClient>::new();

    Ok(())
}

fn get_env(name: &str) -> eyre::Result<String> {
    env::var(name).wrap_err_with(|| format!("expected a {} in the environment", name))
}
