use std::{
    ops::DerefMut,
    path::{Path, PathBuf},
};

use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tap::{TapFallible, TapOptional};
use thiserror::Error;
use tokio::{
    sync::{broadcast, mpsc},
    time::Duration,
};
use tracing::{debug, error, info, instrument, trace};
use twitch_irc::{
    login::{LoginCredentials, RefreshingLoginCredentials},
    message::ServerMessage,
    ClientConfig, TCPTransport, Transport, TwitchIRCClient,
};

use crate::{
    auth::SQLiteTokenStore,
    commands::CommandsStore,
    msg::{BuiltInCommand, ImplicitTask, Metadata, Response, Task, WithMeta},
    wordsearch::WordSearch,
};

/// The main `oxbow` bot entry point.
pub struct Bot {
    client_id: String,
    client_secret: String,
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
        let prefix = self.prefix;
        let receive_loop = tokio::spawn(async move {
            let mut handler = ReceiveHandler {
                msg_rx,
                task_tx,
                prefix,
            };

            handler.receive().await;
        });

        // Spawn a processing loop to interpret Tasks and turn them into
        // Responses if necessary.
        let res_tx = res_tx_orig.clone();
        let commands = CommandsStore::new(self.conn_pool.clone());
        let prefix = self.prefix;
        let process_loop = tokio::spawn(async move {
            let mut handler = ProcessHandler {
                task_rx,
                res_tx,
                commands,
                prefix,
                word_search: WordSearch::new(),
            };

            handler.process().await;
        });

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

                handler.respond().await;
            });
        }

        receive_loop.await.unwrap();
        process_loop.await.unwrap();

        Ok(())
    }
}

struct ReceiveHandler {
    msg_rx: mpsc::UnboundedReceiver<ServerMessage>,
    task_tx: mpsc::UnboundedSender<WithMeta<Task>>,
    prefix: char,
}

impl ReceiveHandler {
    /// Loops over incoming messages and if any are a recognised command, sends
    /// a [`Task`] in `task_tx` with the appropriate task to perform.
    #[instrument(skip(self))]
    async fn receive(&mut self) {
        debug!("starting");

        loop {
            trace!("waiting for incoming message");

            let message = self
                .msg_rx
                .recv()
                .await
                .tap_some(|_| trace!("received incoming message"))
                .tap_none(|| error!("failed to receive incoming message"));

            let task = match message {
                Some(ServerMessage::Privmsg(msg)) => {
                    let meta = Metadata { id: msg.message_id };
                    let mut components = msg.message_text.split(' ');

                    if let Some(command) =
                        components.next().and_then(|c| c.strip_prefix(self.prefix))
                    {
                        match command {
                            "command" => {
                                debug!(id = %meta.id, command = "command", "identified command");

                                if let Some(trigger) = components.next() {
                                    let response = components.collect::<Vec<_>>().join(" ");

                                    if !response.is_empty() {
                                        info!(id = %meta.id, ?trigger, ?response, "adding command");

                                        Some(WithMeta(
                                            Task::BuiltIn(BuiltInCommand::AddCommand {
                                                channel: msg.channel_login.to_owned(),
                                                trigger: trigger.to_owned(),
                                                response: response.to_owned(),
                                            }),
                                            meta,
                                        ))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }

                            "search" => {
                                debug!(id = %meta.id, command = "search", "identified command");

                                if msg.sender.login == "nerosnm" {
                                    Some(WithMeta(
                                        Task::BuiltIn(BuiltInCommand::WordSearch {
                                            channel: msg.channel_login.to_owned(),
                                        }),
                                        meta,
                                    ))
                                } else {
                                    None
                                }
                            }

                            "lower" => {
                                debug!(id = %meta.id, command = "lower", "identified command");

                                if msg.sender.login == "nerosnm" {
                                    if let Some(word) = components.next() {
                                        if let Some(distance) =
                                            components.next().and_then(|d| d.parse().ok())
                                        {
                                            Some(WithMeta(
                                                Task::BuiltIn(BuiltInCommand::WordLower {
                                                    channel: msg.channel_login.to_owned(),
                                                    word: word.to_owned(),
                                                    distance,
                                                }),
                                                meta,
                                            ))
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }

                            "upper" => {
                                debug!(id = %meta.id, command = "upper", "identified command");

                                if msg.sender.login == "nerosnm" {
                                    if let Some(word) = components.next() {
                                        if let Some(distance) =
                                            components.next().and_then(|d| d.parse().ok())
                                        {
                                            Some(WithMeta(
                                                Task::BuiltIn(BuiltInCommand::WordUpper {
                                                    channel: msg.channel_login.to_owned(),
                                                    word: word.to_owned(),
                                                    distance,
                                                }),
                                                meta,
                                            ))
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }

                            "found" => {
                                debug!(id = %meta.id, command = "found", "identified command");

                                if msg.sender.login == "nerosnm" {
                                    Some(WithMeta(
                                        Task::BuiltIn(BuiltInCommand::WordFound {
                                            channel: msg.channel_login.to_owned(),
                                        }),
                                        meta,
                                    ))
                                } else {
                                    None
                                }
                            }

                            other => Some(WithMeta(
                                Task::Command {
                                    channel: msg.channel_login.to_owned(),
                                    sender: msg.sender.login.to_owned(),
                                    command: other.to_owned(),
                                    body: components.collect::<Vec<_>>().join(" "),
                                },
                                meta,
                            )),
                        }
                    } else if msg
                        .message_text
                        .to_lowercase()
                        .split_whitespace()
                        .any(|ea| ea == "hi")
                        && msg.message_text.to_lowercase().contains("@oxoboxowot")
                    {
                        trace!(id = %meta.id, implicit_command = "greeting", "implicit command identified");
                        info!(id = %meta.id, ?msg.channel_login, ?msg.sender.login, ?msg.message_text);

                        Some(WithMeta(
                            Task::Implicit(ImplicitTask::Greet {
                                channel: msg.channel_login,
                                user: msg.sender.login,
                            }),
                            meta,
                        ))
                    } else {
                        None
                    }
                }
                Some(_) => None,
                None => None,
            };

            if let Some(WithMeta(task, meta)) = task {
                let id = meta.id.clone();
                let _ = self
                    .task_tx
                    .send(WithMeta(task, meta))
                    .tap_err(|e| error!(%id, error = ?e, "failed to send task message"));
            }
        }
    }
}

struct ProcessHandler {
    task_rx: mpsc::UnboundedReceiver<WithMeta<Task>>,
    res_tx: broadcast::Sender<WithMeta<Response>>,
    commands: CommandsStore,
    prefix: char,
    word_search: WordSearch,
}

impl ProcessHandler {
    /// Loops over incoming [`Task`]s, acts on them, and if necessary, sends a
    /// [`Response`] in `res_tx` with the appropriate response to send.
    #[instrument(skip(self))]
    async fn process(&mut self) {
        debug!("starting");

        loop {
            trace!("waiting for task message");

            let task = self
                .task_rx
                .recv()
                .await
                .tap_some(|_| trace!("received task message"))
                .tap_none(|| error!("failed to receive task message"));

            let response = match task {
                Some(WithMeta(
                    Task::Command {
                        channel,
                        sender: _,
                        command,
                        body: _,
                    },
                    meta,
                )) => self
                    .commands
                    .get_command(&channel, &command)
                    .expect("getting a command should succeed")
                    .map(|response| {
                        WithMeta(
                            Response::Say {
                                channel,
                                message: response,
                            },
                            meta,
                        )
                    }),
                Some(WithMeta(Task::Implicit(ImplicitTask::Greet { channel, user }), meta)) => {
                    info!(id = %meta.id, ?channel, ?user, "implicit greet task");

                    Some(WithMeta(
                        Response::Say {
                            channel,
                            message: format!("uwu *nuzzles @{}*", user),
                        },
                        meta,
                    ))
                }
                Some(WithMeta(
                    Task::BuiltIn(BuiltInCommand::AddCommand {
                        channel,
                        trigger,
                        response,
                    }),
                    meta,
                )) => {
                    info!(id = %meta.id, ?trigger, ?response, "add command task");

                    let already_exists = self
                        .commands
                        .get_command(&channel, &trigger)
                        .expect("getting a command should succeed")
                        .is_some();

                    self.commands
                        .set_command(&channel, &trigger, &response)
                        .expect("setting a command should succeed");

                    let verb = if already_exists { "Updated" } else { "Added" };

                    Some(WithMeta(
                        Response::Say {
                            channel,
                            message: format!("{} {}{}", verb, self.prefix, trigger),
                        },
                        meta,
                    ))
                }
                Some(WithMeta(Task::BuiltIn(BuiltInCommand::WordSearch { channel }), meta)) => {
                    info!(id = %meta.id, "word search task");

                    self.word_search.reset();

                    Some(WithMeta(
                        Response::Say {
                            channel,
                            message: format!("!wg {}", self.word_search.guess()),
                        },
                        meta,
                    ))
                }
                Some(WithMeta(
                    Task::BuiltIn(BuiltInCommand::WordLower {
                        channel,
                        word,
                        distance,
                    }),
                    meta,
                )) => {
                    info!(id = %meta.id, ?word, "word lower task");

                    self.word_search.set_lower(&word, distance);

                    Some(WithMeta(
                        Response::Say {
                            channel,
                            message: format!("!wg {}", self.word_search.guess()),
                        },
                        meta,
                    ))
                }
                Some(WithMeta(
                    Task::BuiltIn(BuiltInCommand::WordUpper {
                        channel,
                        word,
                        distance,
                    }),
                    meta,
                )) => {
                    info!(id = %meta.id, ?word, "word upper task");

                    self.word_search.set_upper(&word, distance);

                    Some(WithMeta(
                        Response::Say {
                            channel,
                            message: format!("!wg {}", self.word_search.guess()),
                        },
                        meta,
                    ))
                }
                Some(WithMeta(Task::BuiltIn(BuiltInCommand::WordFound { channel }), meta)) => {
                    info!(id = %meta.id, "word found task");

                    Some(WithMeta(
                        Response::Say {
                            channel,
                            message: format!("Word search stopped"),
                        },
                        meta,
                    ))
                }
                None => None,
            };

            if let Some(WithMeta(res, meta)) = response {
                let id = meta.id.clone();
                let _ = self
                    .res_tx
                    .send(WithMeta(res, meta))
                    .tap_err(|e| error!(%id, error = ?e, "failed to send response message"));
            }
        }
    }
}

struct RespondHandler<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    res_rx: broadcast::Receiver<WithMeta<Response>>,
    client: TwitchIRCClient<T, L>,
    channel: String,
}

impl<T, L> RespondHandler<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    /// Watches for relevant messages coming in through `msg_rx` and acts on
    /// them in `channel`, such as sending responses.
    #[instrument(skip(self))]
    async fn respond(&mut self) {
        debug!("starting");

        self.client.join(self.channel.clone());

        while self.client.get_channel_status(self.channel.clone()).await != (true, true) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("joined channel");

        loop {
            trace!("waiting for response message");

            let res = self
                .res_rx
                .recv()
                .await
                .tap_ok(|_| trace!("received response message"))
                .tap_err(|e| error!(error = ?e, "failed to receive response message"));

            let response = match res {
                Ok(WithMeta(
                    Response::Say {
                        channel: msg_channel,
                        message,
                    },
                    meta,
                )) if msg_channel == self.channel => {
                    info!(id = %meta.id, response.channel = ?msg_channel, response.message = ?message, "sending response");

                    Some(WithMeta((msg_channel, message), meta))
                }
                Ok(WithMeta(
                    Response::Say {
                        channel: _,
                        message: _,
                    },
                    _,
                )) => None,
                Err(_) => None,
            };

            if let Some(WithMeta((channel, message), meta)) = response {
                let _ = self
                    .client
                    .say(channel, message)
                    .await
                    .tap_err(|e| error!(id = %meta.id, error = ?e, "unable to send response"));
            }
        }
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
    prefix: Option<char>,
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

    /// Set the prefix that commands start with.
    pub fn prefix(mut self, prefix: char) -> Self {
        self.prefix = Some(prefix);
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
        let prefix = self.prefix.ok_or(BotBuildError::NoPrefix)?;

        let manager = self.db_path.map_or_else(
            SqliteConnectionManager::memory,
            SqliteConnectionManager::file,
        );

        let conn_pool = Pool::new(manager)?;

        Ok(Bot {
            client_id,
            client_secret,
            twitch_name,
            channels,
            prefix,
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

    #[error("no channels to join provided")]
    NoChannels,

    #[error("no prefix provided")]
    NoPrefix,

    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),
}
