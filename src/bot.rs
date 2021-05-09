use std::path::{Path, PathBuf};

use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, instrument};
use twitch_irc::{
    login::{LoginCredentials, RefreshingLoginCredentials},
    message::ServerMessage,
    ClientConfig, TCPTransport, Transport, TwitchIRCClient,
};

use crate::{
    auth::SQLiteTokenStore,
    msg::{ImplicitTask, Response, Task},
};

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

        // Channel for the receive loop to trigger tasks in the process loop.
        let (task_tx, task_rx) = mpsc::unbounded_channel();

        // Channel for the process loop to trigger responses in the response
        // loops.
        let (res_tx, _) = broadcast::channel(16);

        // Spawn a receive loop to interpret incoming messages and turn them
        // into Tasks if necessary.
        let receive_loop = tokio::spawn(Self::receive(incoming_messages, task_tx.clone()));

        // Spawn a processing loop to interpret Tasks and turn them into
        // Responses if necessary.
        let process_loop = tokio::spawn(Self::process(
            task_rx,
            res_tx.clone(),
            self.conn_pool.clone(),
        ));

        // For every channel, we need a response loop to perform Responses if
        // they're relevant to that channel.
        for channel in self.channels.iter() {
            tokio::spawn(Self::respond(
                client.clone(),
                channel.to_owned(),
                res_tx.subscribe(),
            ));
        }

        receive_loop.await.unwrap();
        process_loop.await.unwrap();

        Ok(())
    }

    /// Loops over incoming messages and if any are a recognised command, sends
    /// a [`Task`] in `task_tx` with the appropriate task to perform.
    #[instrument(skip(incoming, task_tx))]
    async fn receive(
        mut incoming: mpsc::UnboundedReceiver<ServerMessage>,
        task_tx: mpsc::UnboundedSender<Task>,
    ) {
        while let Some(message) = incoming.recv().await {
            match message {
                ServerMessage::Privmsg(msg) => {
                    if msg.message_text.contains("hi") && msg.message_text.contains("@oxoboxowot") {
                        info!(?msg.channel_login, ?msg.sender.login, ?msg.message_text);

                        task_tx
                            .send(Task::Implicit(ImplicitTask::Greet {
                                channel: msg.channel_login,
                                user: msg.sender.login,
                            }))
                            .expect("sending tasks should succeed");
                    }
                }
                _ => (),
            }
        }
    }

    /// Loops over incoming [`Task`]s, acts on them, and if necessary, sends a
    /// [`Response`] in `res_tx` with the appropriate response to send.
    #[instrument(skip(task_rx, res_tx, _pool))]
    async fn process(
        mut task_rx: mpsc::UnboundedReceiver<Task>,
        res_tx: broadcast::Sender<Response>,
        _pool: Pool<SqliteConnectionManager>,
    ) {
        loop {
            let task = task_rx
                .recv()
                .await
                .expect("receiving tasks should succeed");

            match task {
                Task::Implicit(ImplicitTask::Greet { channel, user }) => {
                    res_tx
                        .send(Response::Say {
                            channel,
                            message: format!("uwu *nuzzles @{}*", user),
                        })
                        .expect("sending messages should succeed");
                }
                _ => (),
            }
        }
    }

    /// Watches for relevant messages coming in through `msg_rx` and acts on
    /// them in `channel`, such as sending responses.
    #[instrument(skip(client, res_rx))]
    async fn respond<T, L>(
        client: TwitchIRCClient<T, L>,
        channel: String,
        mut res_rx: broadcast::Receiver<Response>,
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
            let res = res_rx
                .recv()
                .await
                .expect("receiving responses should succeed");

            match res {
                Response::Say {
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
