use std::path::{Path, PathBuf};

use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc::UnboundedReceiver};
use tracing::{debug, info, instrument};
use twitch_irc::{
    login::{LoginCredentials, RefreshingLoginCredentials},
    message::ServerMessage,
    ClientConfig, TCPTransport, Transport, TwitchIRCClient,
};

use crate::{auth::SQLiteTokenStore, msg::Message};

/// The main `oxbow` bot entry point.
pub struct Bot {
    client_id: String,
    client_secret: String,
    twitch_name: String,
    channels: Vec<String>,
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
    pub async fn run(&mut self) -> Result<(), BotError> {
        let mut store = SQLiteTokenStore::new(self.conn_pool.clone());
        let _ = crate::auth::authenticate(&mut store, &self.client_id, &self.client_secret).await?;

        let creds = RefreshingLoginCredentials::new(
            self.twitch_name.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
            store,
        );
        let config = ClientConfig::new_simple(creds);

        let (incoming_messages, client) = TwitchIRCClient::<
            TCPTransport,
            RefreshingLoginCredentials<SQLiteTokenStore>,
        >::new(config);

        let (tx, _rx) = broadcast::channel::<Message>(16);
        let tx1 = tx.clone();

        let receive_loop = tokio::spawn(Self::receive(incoming_messages, tx1));

        for channel in self.channels.iter() {
            tokio::spawn(Self::respond(
                client.clone(),
                channel.to_owned(),
                tx.subscribe(),
            ));
        }

        receive_loop.await.unwrap();

        Ok(())
    }

    /// Loops over incoming messages and if any are a recognised command, sends
    /// a message in `msg_tx` with the appropriate action to take.
    #[instrument(skip(incoming, msg_tx))]
    async fn receive(
        mut incoming: UnboundedReceiver<ServerMessage>,
        msg_tx: broadcast::Sender<Message>,
    ) {
        while let Some(message) = incoming.recv().await {
            match message {
                ServerMessage::Privmsg(msg) => {
                    if &*msg.message_text == "hi @oxoboxowot" {
                        info!(?msg.channel_login, ?msg.sender.login, ?msg.message_text);

                        msg_tx
                            .send(Message::Response {
                                channel: msg.channel_login.clone(),
                                message: format!("uwu @{} *nuzzles you*", msg.sender.login),
                            })
                            .expect("sending messages should succeed");
                    }
                }
                _ => (),
            }
        }
    }

    /// Watches for relevant messages coming in through `msg_rx` and acts on
    /// them in `channel`, such as sending responses.
    #[instrument(skip(client, msg_rx))]
    async fn respond<T, L>(
        client: TwitchIRCClient<T, L>,
        channel: String,
        mut msg_rx: broadcast::Receiver<Message>,
    ) where
        T: Transport,
        L: LoginCredentials,
    {
        client.join(channel.clone());

        while client.get_channel_status(channel.clone()).await != (true, true) {
            continue;
        }

        info!("joined channel");

        loop {
            let msg = msg_rx
                .recv()
                .await
                .expect("receiving messages should succeed");

            match msg {
                Message::Response {
                    channel: msg_channel,
                    message,
                } if msg_channel == channel => {
                    info!(response.channel = ?msg_channel, response.message = ?message, "sending response");

                    client
                        .say(msg_channel, message)
                        .await
                        .expect("unable to send response");
                }
                _ => (),
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum BotError {
    #[error("migration error: {0}")]
    Migration(#[source] refinery::Error),

    #[error("authentication error: {0}")]
    Auth(#[from] crate::auth::AuthError),
}

/// The number one single when Twitch user @NinthRoads was born was Bob The
/// Builder.
pub type BotTheBuilder = BotBuilder;

/// Builder for an instance of [`Bot`].
#[derive(Default)]
pub struct BotBuilder {
    client_id: Option<String>,
    client_secret: Option<String>,
    twitch_name: Option<String>,
    channels: Option<Vec<String>>,
    db_path: Option<PathBuf>,
}

impl BotBuilder {
    /// Set the client ID this bot will use for authentication.
    pub fn client_id<S: ToString>(mut self, client_id: S) -> Self {
        self.client_id = Some(client_id.to_string());
        self
    }

    /// Set the client secret this bot will use for authentication.
    pub fn client_secret<S: ToString>(mut self, client_secret: S) -> Self {
        self.client_secret = Some(client_secret.to_string());
        self
    }

    /// Set the Twitch username of this bot.
    pub fn twitch_name<S: ToString>(mut self, twitch_name: S) -> Self {
        self.twitch_name = Some(twitch_name.to_string());
        self
    }

    /// Add a channel to the list of channels to join.
    pub fn add_channel<S: ToString>(mut self, channel: S) -> Self {
        self.channels
            .get_or_insert_with(Vec::new)
            .push(channel.to_string());
        self
    }

    /// Extend the list of channels to join with an iterator.
    pub fn extend_channels<I, S>(mut self, channels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: ToString,
    {
        self.channels
            .get_or_insert_with(Vec::new)
            .extend(channels.into_iter().map(|s| s.to_string()));

        self
    }

    /// Set the bot to attempt to open the SQLite3 database at `path` and use
    /// that as its database, instead of using an in-memory database.
    pub fn db_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.db_path = Some(path.as_ref().to_owned());
        self
    }

    /// Create a [`Bot`] from this builder, validating the provided values.
    pub fn build(self) -> Result<Bot, BotBuildError> {
        let client_id = self.client_id.ok_or(BotBuildError::NoClientId)?;
        let client_secret = self.client_secret.ok_or(BotBuildError::NoClientSecret)?;
        let twitch_name = self.twitch_name.ok_or(BotBuildError::NoTwitchName)?;
        let channels = self.channels.ok_or(BotBuildError::NoChannels)?;

        let manager = self
            .db_path
            .map_or_else(
                SqliteConnectionManager::memory,
                SqliteConnectionManager::file,
            )
            .with_init(|conn| {
                let report = crate::db::migrations::runner()
                    .run(conn)
                    .expect("running migrations should succeed");

                debug!(?report);

                Ok(())
            });

        let conn_pool = Pool::new(manager)?;

        Ok(Bot {
            client_id,
            client_secret,
            twitch_name,
            channels,
            conn_pool,
        })
    }
}

#[derive(Debug, Error)]
pub enum BotBuildError {
    #[error("no client ID provided")]
    NoClientId,

    #[error("no client secret provided")]
    NoClientSecret,

    #[error("no twitch name provided")]
    NoTwitchName,

    #[error("no channels to join specified")]
    NoChannels,

    #[error("no information about a database specified")]
    NoDbInfo,

    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),
}
