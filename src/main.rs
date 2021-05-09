use clap::Clap;
use eyre::Result;
use opts::Opts;
use oxbow::Bot;
use surf::Client as SurfClient;
use twitch_api2::TwitchClient;

mod opts;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenv::dotenv().ok();

    let opts: Opts = Opts::parse();

    let mut bot_the_builder = Bot::the_builder()
        .client_id(opts.client_id)
        .client_secret(opts.client_secret)
        .twitch_name(opts.twitch_name)
        .extend_channels(opts.channels)
        .prefix(opts.prefix);

    if let Some(db_path) = opts.database {
        bot_the_builder = bot_the_builder.db_path(db_path);
    }

    bot_the_builder.build()?.run().await?;

    let _client = TwitchClient::<'_, SurfClient>::new();

    Ok(())
}
