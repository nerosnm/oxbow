use clap::Parser;

#[derive(Parser, Debug)]
pub struct Opts {
    /// Path to the database file, such as `db.sqlite3`, to use. If this is not
    /// provided, an in-memory database will be used instead.
    #[clap(short, long, env = "DATABASE", hide_env_values = true)]
    pub database: Option<String>,

    /// The client ID to use for authentication with the Twitch API.
    #[clap(long = "id", env = "CLIENT_ID", hide_env_values = true)]
    pub client_id: String,

    /// The client secret to use for authentication with the Twitch API.
    #[clap(long = "secret", env = "CLIENT_SECRET", hide_env_values = true)]
    pub client_secret: String,

    /// The username of the account to post as in Twitch chat.
    #[clap(
        short = 'n',
        long = "name",
        env = "TWITCH_NAME",
        hide_env_values = true
    )]
    pub twitch_name: String,

    /// The character that commands start with.
    #[clap(long, default_value = "!")]
    pub prefix: char,

    /// A space-separated list of channels to join.
    pub channels: Vec<String>,
}
