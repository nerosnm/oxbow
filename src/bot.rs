use std::{collections::HashMap, ops::DerefMut};

use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tracing::debug;
use twitch_irc::{login::RefreshingLoginCredentials, ClientConfig, TCPTransport, TwitchIRCClient};

use crate::{auth::SQLiteTokenStore, commands::CommandsStore, settings::Settings};

mod handler;

#[cfg(feature = "obs")]
pub use self::handler::ObsHandler;
pub use self::handler::{ProcessHandler, ReceiveHandler, RespondHandler};

/// The main `oxbow` bot entry point.
pub struct Bot {
    client_id: String,
    client_secret: String,
    twitch_name: String,
    #[cfg(feature = "obs")]
    obs_port: u16,
    #[cfg(feature = "obs")]
    obs_password: String,
    channels: Vec<String>,
    prefix: String,
    conn_pool: Pool<SqliteConnectionManager>,
}

impl Bot {
    pub fn new(settings: Settings) -> Result<Self, BotError> {
        let manager = settings.database_path.map_or_else(
            SqliteConnectionManager::memory,
            SqliteConnectionManager::file,
        );

        let conn_pool = Pool::new(manager)?;

        Ok(Self {
            client_id: settings.twitch.client_id,
            client_secret: settings.twitch.client_secret,
            twitch_name: settings.twitch.name,
            #[cfg(feature = "obs")]
            obs_port: settings.obs.websocket_port,
            #[cfg(feature = "obs")]
            obs_password: settings.obs.websocket_password,
            channels: settings.twitch.channels,
            prefix: settings.prefix,
            conn_pool,
        })
    }

    /// Main run loop for the bot.
    ///
    /// Spawns tasks to receive messages, and to send messages to each connected
    /// channel.
    pub async fn run(&mut self) -> Result<(), BotError> {
        let mut conn = self.conn_pool.get()?;
        let report = crate::db::migrations::runner().run(conn.deref_mut())?;
        debug!(?report);

        let mut store = SQLiteTokenStore::new(self.conn_pool.clone());
        crate::auth::authenticate(&mut store, &self.client_id, &self.client_secret).await?;

        let creds = RefreshingLoginCredentials::new(
            self.twitch_name.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
            store,
        );
        let config = ClientConfig::new_simple(creds);
        let (msg_rx, client) = TwitchIRCClient::<TCPTransport, _>::new(config);

        // Channel for the receive loop to trigger tasks in the process loop.
        let (task_tx, task_rx) = mpsc::unbounded_channel();

        // Channel for the process loop to trigger responses in the response
        // loops.
        let (res_tx_orig, _) = broadcast::channel(16);

        // Spawn a receive loop to interpret incoming messages and turn them
        // into Tasks if necessary.
        let prefix = self.prefix.clone();
        let receive_loop = tokio::spawn(async move {
            let mut handler = ReceiveHandler {
                msg_rx,
                task_tx,
                prefix,
            };

            handler.receive_loop().await;
        });

        // Spawn a processing loop to interpret Tasks and turn them into
        // Responses if necessary.
        let res_tx = res_tx_orig.clone();
        let commands = CommandsStore::new(self.conn_pool.clone());
        let prefix = self.prefix.clone();
        let process_loop = tokio::spawn(async move {
            let mut handler = ProcessHandler {
                task_rx,
                res_tx,
                commands,
                prefix,
                word_searches: HashMap::new(),
            };

            handler.process_loop().await;
        });

        #[cfg(feature = "obs")]
        {
            // We need a loop to interact with OBS.
            let port = self.obs_port;
            let password = self.obs_password.clone();
            tokio::spawn(async move {
                let mut handler = ObsHandler { port, password };

                handler.obs_loop().await;
            });
        }

        // For every channel, we need a response loop to perform Responses if
        // they're relevant to that channel.
        for channel in self.channels.iter() {
            let res_rx = res_tx_orig.subscribe();
            let client = client.clone();
            let channel = channel.to_owned();

            tokio::spawn(async move {
                let mut handler = RespondHandler {
                    res_rx,
                    client,
                    channel,
                };

                handler.respond_loop().await;
            });
        }

        receive_loop.await.unwrap();
        process_loop.await.unwrap();

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum BotError {
    #[error("migration error: {0}")]
    Migration(#[from] refinery::Error),

    #[error("authentication error: {0}")]
    Auth(#[from] crate::auth::AuthError),

    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),
}
