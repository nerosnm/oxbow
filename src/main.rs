use clap::Parser;
use eyre::Result;
use opts::Opts;
use oxbow::bot::Bot;

mod opts;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    dotenv::dotenv().ok();

    let opts: Opts = Opts::parse();

    let mut bot_the_builder = Bot::the_builder()
        .twitch_credentials(opts.client_id, opts.client_secret)
        .twitch_name(opts.twitch_name)
        .extend_channels(opts.channels)
        .prefix(opts.prefix);

    if let Some(db_path) = opts.database {
        bot_the_builder = bot_the_builder.db_path(db_path);
    }

    bot_the_builder.build()?.run().await?;

    Ok(())
}
