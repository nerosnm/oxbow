use std::collections::HashMap;

use clap::Clap;
use config::{ConfigError, Source, Value};

#[derive(Clap, Clone, Debug)]
pub struct Opts {
    /// The string that commands start with.
    #[clap(long, default_value = "!")]
    pub prefix: String,

    /// Path to the database file, such as `db.sqlite3`, to use. If this is not
    /// provided, an in-memory database will be used instead.
    #[clap(short, long, env = "DATABASE", hide_env_values = true)]
    pub database: Option<String>,

    /// The client ID to use for authentication with the Twitch API.
    #[clap(long = "id", env = "CLIENT_ID", hide_env_values = true)]
    pub client_id: Option<String>,

    /// The client secret to use for authentication with the Twitch API.
    #[clap(long = "secret", env = "CLIENT_SECRET", hide_env_values = true)]
    pub client_secret: Option<String>,

    /// The username of the account to post as in Twitch chat.
    #[clap(
        short = 'n',
        long = "name",
        env = "TWITCH_NAME",
        hide_env_values = true
    )]
    pub twitch_name: Option<String>,

    /// A comma-separated list of Twitch chat channels to join.
    #[clap(short = 'c', long = "channels")]
    pub channels: Option<Vec<String>>,

    /// The port of the OBS websocket.
    #[clap(long = "obs-port", env = "OBS_PORT", hide_env_values = true)]
    #[cfg(feature = "obs")]
    pub obs_port: Option<i64>,

    /// The password for the OBS websocket.
    #[clap(long = "obs-password", env = "OBS_PASSWORD", hide_env_values = true)]
    #[cfg(feature = "obs")]
    pub obs_password: Option<String>,
}

impl Source for Opts {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<HashMap<String, Value>, ConfigError> {
        let mut values = HashMap::<String, Value>::new();

        values.insert("prefix".into(), self.prefix.into());

        if let Some(db) = self.database {
            values.insert("database".into(), db.into());
        }

        let mut twitch = HashMap::<String, Value>::new();

        if let Some(ref client_id) = self.client_id {
            twitch.insert("client_id".into(), (*client_id).into());
        }

        if let Some(ref client_secret) = self.client_secret.as_ref() {
            twitch.insert("client_secret".into(), (*client_secret).into());
        }

        if let Some(ref twitch_name) = self.twitch_name {
            twitch.insert("name".into(), twitch_name.into());
        }

        if let Some(ref channels) = self.channels {
            twitch.insert("channels".into(), channels.into());
        }

        if !twitch.is_empty() {
            values.insert("twitch".into(), twitch.into());
        }

        #[cfg(feature = "obs")]
        {
            let mut obs = HashMap::<String, Value>::new();

            if let Some(obs_port) = self.obs_port {
                obs.insert("websocket_port".into(), obs_port.into());
            }

            if let Some(ref obs_password) = self.obs_password {
                obs.insert("websocket_password".into(), obs_password.into());
            }

            if !obs.is_empty() {
                values.insert("obs".into(), obs.into());
            }
        }

        Ok(values)
    }
}
