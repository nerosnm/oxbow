use std::env;

use eyre::{Result, WrapErr};
use surf::Client as SurfClient;
use tracing::info;
use twitch_api2::TwitchClient;
use twitch_irc::{login::StaticLoginCredentials, ClientConfig, TCPTransport, TwitchIRCClient};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenv::dotenv().ok();

    let client_id = get_env("CLIENT_ID")?;
    let client_secret = get_env("CLIENT_SECRET")?;
    let twitch_name = get_env("TWITCH_NAME")?;
    let twitch_channel = get_env("TWITCH_CHANNEL")?;

    let token = oxbow::auth::authenticate(&client_id, &client_secret).await;

    let _client = TwitchClient::<'_, SurfClient>::new();

    let token = token.access_token.secret().to_string();
    let creds = StaticLoginCredentials::new(twitch_name, Some(token));
    let config = ClientConfig::new_simple(creds);

    let (mut incoming_messages, client) =
        TwitchIRCClient::<TCPTransport, StaticLoginCredentials>::new(config);

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
