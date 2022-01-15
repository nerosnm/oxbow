use std::iter;

use tap::Pipe;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, trace};
use twitch_irc::message::ServerMessage;

use crate::{
    msg::{BuiltInCommand, Help, ImplicitTask, Metadata, Task, WithMeta},
    parse::{
        ast::{Command, Help as AstHelp, MetaCommand, PotentialUser, Quote, Search},
        oxbow::CommandParser,
    },
};

pub struct ReceiveHandler {
    pub(in crate::bot) msg_rx: mpsc::UnboundedReceiver<ServerMessage>,
    pub(in crate::bot) task_tx: mpsc::UnboundedSender<(Task, Metadata)>,
    pub(in crate::bot) prefix: char,
    pub(in crate::bot) twitch_name: String,
    pub(in crate::bot) parser: CommandParser,
}

impl ReceiveHandler {
    /// Loops over incoming messages and if any are a recognised command, sends
    /// a [`Task`] in `task_tx` with the appropriate task to perform.
    #[instrument(skip(self))]
    pub async fn receive_loop(&mut self) {
        debug!("starting");

        loop {
            match self.receive().await {
                Ok(()) => {}
                Err(err) => error!(%err),
            }
        }
    }

    /// Gets an incoming message, and if it is a recognised command, sends
    /// [`Task`]s in `task_tx` with the appropriate tasks to perform.
    #[instrument(skip(self))]
    async fn receive(&mut self) -> Result<(), ReceiveError> {
        trace!("waiting for incoming message");

        let message = self
            .msg_rx
            .recv()
            .await
            .ok_or(ReceiveError::ReceiveMessage)?;

        trace!("received incoming message");

        for (task, meta) in self.handle_message(message).await? {
            self.send_task(task, meta).await?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_message(
        &mut self,
        message: ServerMessage,
    ) -> Result<Vec<(Task, Metadata)>, ReceiveError> {
        let tasks = match message {
            ServerMessage::Privmsg(msg) => {
                let meta = Metadata {
                    id: msg.message_id.into(),
                    channel: msg.channel_login.into(),
                    sender: msg.sender.login.into(),
                };

                if let Some(potential_command) = msg.message_text.strip_prefix(self.prefix) {
                    if let Ok(parsed) = self.parser.parse(potential_command) {
                        match parsed {
                            Command::Quote(Quote::Add {
                                username,
                                key,
                                text,
                            }) => {
                                debug!(?meta, command = "add quote", "identified command");
                                Task::BuiltIn(BuiltInCommand::AddQuote {
                                    username,
                                    key,
                                    text,
                                })
                                .with_meta(meta)
                                .pipe(iter::once)
                                .collect()
                            }
                            Command::Quote(Quote::Get { key }) => {
                                debug!(?meta, command = "get quote by key", "identified command");
                                Task::BuiltIn(BuiltInCommand::GetQuote { key })
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                            }
                            Command::Quote(Quote::Random) => {
                                debug!(?meta, command = "get random quote", "identified command");
                                Task::BuiltIn(BuiltInCommand::RandomQuote)
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                            }
                            Command::Help(AstHelp::General) => {
                                debug!(?meta, "identified general help request");
                                Task::Help(Help::General)
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                            }
                            Command::Help(AstHelp::Quote) => {
                                debug!(
                                    ?meta,
                                    command = "quote",
                                    "identified help request for command"
                                );
                                Task::Help(Help::Quote)
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                            }
                            Command::Meta(MetaCommand { trigger, response }) => {
                                debug!(?meta, command = "command", "identified command");
                                Task::BuiltIn(BuiltInCommand::AddCommand { trigger, response })
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                            }
                            Command::Search(Search::Search) => {
                                debug!(?meta, command = "search", "identified command");
                                if &*meta.sender == "nerosnm" {
                                    Task::BuiltIn(BuiltInCommand::WordSearch)
                                        .with_meta(meta)
                                        .pipe(iter::once)
                                        .collect()
                                } else {
                                    iter::empty().collect()
                                }
                            }
                            Command::Search(Search::Lower { word, distance }) => {
                                debug!(?meta, command = "lower", "identified command");

                                if &*meta.sender == "nerosnm" {
                                    Task::BuiltIn(BuiltInCommand::WordLower { word, distance })
                                        .with_meta(meta)
                                        .pipe(iter::once)
                                        .collect()
                                } else {
                                    iter::empty().collect()
                                }
                            }
                            Command::Search(Search::Upper { word, distance }) => {
                                debug!(?meta, command = "upper", "identified command");

                                if &*meta.sender == "nerosnm" {
                                    Task::BuiltIn(BuiltInCommand::WordUpper { word, distance })
                                        .with_meta(meta)
                                        .pipe(iter::once)
                                        .collect()
                                } else {
                                    iter::empty().collect()
                                }
                            }
                            Command::Search(Search::Found) => {
                                debug!(?meta, command = "found", "identified command");

                                if &*meta.sender == "nerosnm" {
                                    Task::BuiltIn(BuiltInCommand::WordFound)
                                        .with_meta(meta)
                                        .pipe(iter::once)
                                        .collect()
                                } else {
                                    iter::empty().collect()
                                }
                            }
                            Command::PotentialUser(PotentialUser { trigger }) => {
                                Task::Command { command: trigger }
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                            }
                        }
                    } else {
                        iter::empty().collect()
                    }
                } else if msg
                    .message_text
                    .to_lowercase()
                    .split_whitespace()
                    .any(|ea| ea == "hi")
                    && msg
                        .message_text
                        .to_lowercase()
                        .contains(&format!("@{}", self.twitch_name))
                {
                    trace!(
                        ?meta,
                        implicit_command = "greeting",
                        "implicit command identified"
                    );
                    info!(?meta, ?msg.message_text);

                    Task::Implicit(ImplicitTask::Greet)
                        .with_meta(meta)
                        .pipe(iter::once)
                        .collect()
                } else {
                    iter::empty().collect()
                }
            }
            ServerMessage::Notice(notice)
                if notice
                    .message_id
                    .as_ref()
                    .map(|id| id.starts_with("msg_"))
                    .unwrap_or(false) =>
            {
                error!(notice = %notice.message_text);
                iter::empty().collect()
            }
            msg => {
                trace!(?msg);
                iter::empty().collect()
            }
        };

        Ok(tasks)
    }

    #[instrument(skip(self))]
    async fn send_task(&mut self, task: Task, meta: Metadata) -> Result<(), ReceiveError> {
        let _ = self.task_tx.send(task.with_cloned_meta(&meta))?;

        Ok(())
    }
}

#[derive(Debug, Error)]
enum ReceiveError {
    #[error("failed to receive message")]
    ReceiveMessage,

    #[error("failed to send task: {0}")]
    SendTask(#[from] mpsc::error::SendError<(Task, Metadata)>),
}
