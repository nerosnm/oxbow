use std::env;

use eyre::{Result, WrapErr};
use oxbow::auth::SQLiteTokenStore;
use rusqlite::Connection;
use surf::Client as SurfClient;
use tracing::info;
use twitch_api2::TwitchClient;
use twitch_irc::{login::RefreshingLoginCredentials, ClientConfig, TCPTransport, TwitchIRCClient};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenv::dotenv().ok();

    let mut conn = Connection::open("./oxbow.sqlite3")?;
    oxbow::db::migrations::runner().run(&mut conn)?;

    let client_id = get_env("CLIENT_ID")?;
    let client_secret = get_env("CLIENT_SECRET")?;
    let twitch_name = get_env("TWITCH_NAME")?;
    let twitch_channel = get_env("TWITCH_CHANNEL")?;

    let mut store = SQLiteTokenStore::new(conn);
    let _ = oxbow::auth::authenticate(&mut store, &client_id, &client_secret).await?;

    let _client = TwitchClient::<'_, SurfClient>::new();

    let creds = RefreshingLoginCredentials::new(twitch_name, client_id, client_secret, store);
    let config = ClientConfig::new_simple(creds);

    let (mut incoming_messages, client) =
        TwitchIRCClient::<TCPTransport, RefreshingLoginCredentials<SQLiteTokenStore>>::new(config);

    let join_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            info!(?message, "received");
        }
    });

    client.join(twitch_channel.clone());

    while client.get_channel_status(twitch_channel.clone()).await != (true, true) {
        continue;
    }

    info!("joined channel {}", twitch_channel.clone());

    info!("sending greeting");
    client.say(twitch_channel.clone(), "uwu".to_owned()).await?;

    join_handle.await.unwrap();

    Ok(())
}

fn get_env(name: &str) -> eyre::Result<String> {
    env::var(name).wrap_err_with(|| format!("expected a {} in the environment", name))
}
