mod builder;
mod handler;

use std::{collections::HashMap, ops::DerefMut};

use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, instrument};
use twitch_irc::{login::RefreshingLoginCredentials, ClientConfig, TCPTransport, TwitchIRCClient};

pub use self::{
    builder::{BotBuilder, BotTheBuilder},
    handler::{ProcessHandler, ReceiveHandler, RespondHandler},
};
use crate::{
    auth::SQLiteTokenStore, commands::CommandsStore, parse::oxbow::CommandParser,
    quotes::QuotesStore,
};

/// The main `oxbow` bot entry point.
pub struct Bot {
    twitch_client_id: String,
    twitch_client_secret: String,
    twitch_name: String,
    channels: Vec<String>,
    prefix: char,
    conn_pool: Pool<SqliteConnectionManager>,
}

impl Bot {
    /// Create a [`BotBuilder`] to build an instance of [`Bot`].
    pub fn builder() -> BotBuilder {
        BotBuilder::default()
    }

    /// Create [`BotTheBuilder`] to build an instance of [`Bot`].
    pub fn the_builder() -> BotTheBuilder {
        BotTheBuilder::default()
    }

    /// Main run loop for the bot.
    ///
    /// Spawns tasks to receive messages, and to send messages to each connected
    /// channel.
    #[instrument(skip(self), fields(channels = ?self.channels, twitch_name = %self.twitch_name, prefix = %self.prefix))]
    pub async fn run(&mut self) -> Result<(), BotError> {
        info!("starting bot");

        let mut conn = self.conn_pool.get()?;
        let report = crate::db::migrations::runner().run(conn.deref_mut())?;
        debug!(?report);

        let mut store = SQLiteTokenStore::new(self.conn_pool.clone());
        crate::auth::authenticate(
            &mut store,
            &self.twitch_client_id,
            &self.twitch_client_secret,
        )
        .await?;

        let creds = RefreshingLoginCredentials::new(
            self.twitch_name.clone(),
            self.twitch_client_id.clone(),
            self.twitch_client_secret.clone(),
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
        let prefix = self.prefix;
        let twitch_name = self.twitch_name.clone();
        let receive_loop = tokio::spawn(async move {
            let mut handler = ReceiveHandler {
                msg_rx,
                task_tx,
                prefix,
                twitch_name,
                parser: CommandParser::new(),
            };

            handler.receive_loop().await;
        });

        // Spawn a processing loop to interpret Tasks and turn them into
        // Responses if necessary.
        let res_tx = res_tx_orig.clone();
        let commands = CommandsStore::new(self.conn_pool.clone());
        let quotes = QuotesStore::new(self.conn_pool.clone());
        let prefix = self.prefix;
        let process_loop = tokio::spawn(async move {
            let mut handler = ProcessHandler {
                task_rx,
                res_tx,
                commands,
                quotes,
                prefix,
                word_searches: HashMap::new(),
            };

            handler.process_loop().await;
        });

        // For every channel, we need a response loop to perform Responses if
        // they're relevant to that channel.
        for channel in self.channels.iter() {
            info!(?channel, "joining channel");

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
