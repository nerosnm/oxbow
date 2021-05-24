use std::iter;

use tap::Pipe;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, trace};
use twitch_irc::message::ServerMessage;

use crate::msg::{BuiltInCommand, ImplicitTask, Metadata, Task, WithMeta};

pub struct ReceiveHandler {
    pub(in crate::bot) msg_rx: mpsc::UnboundedReceiver<ServerMessage>,
    pub(in crate::bot) task_tx: mpsc::UnboundedSender<(Task, Metadata)>,
    pub(in crate::bot) prefix: String,
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

                let mut components = msg.message_text.split(' ');

                if let Some(command) = components.next().and_then(|c| c.strip_prefix(&self.prefix))
                {
                    match command {
                        "command" => {
                            debug!(?meta, command = "command", "identified command");

                            if let Some(trigger) = components.next() {
                                let response = components.collect::<Vec<_>>().join(" ");

                                if !response.is_empty() {
                                    info!(?meta, ?trigger, ?response, "adding command");

                                    Task::BuiltIn(BuiltInCommand::AddCommand {
                                        trigger: trigger.to_owned(),
                                        response: response.to_owned(),
                                    })
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                                } else {
                                    iter::empty().collect()
                                }
                            } else {
                                iter::empty().collect()
                            }
                        }

                        "search" => {
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

                        "lower" => {
                            debug!(?meta, command = "lower", "identified command");

                            if &*meta.sender == "nerosnm" {
                                if let Some(word) = components.next() {
                                    let distance = components.next().and_then(|d| d.parse().ok());

                                    Task::BuiltIn(BuiltInCommand::WordLower {
                                        word: word.to_owned(),
                                        distance,
                                    })
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                                } else {
                                    iter::empty().collect()
                                }
                            } else {
                                iter::empty().collect()
                            }
                        }

                        "upper" => {
                            debug!(?meta, command = "upper", "identified command");

                            if &*meta.sender == "nerosnm" {
                                if let Some(word) = components.next() {
                                    let distance = components.next().and_then(|d| d.parse().ok());

                                    Task::BuiltIn(BuiltInCommand::WordUpper {
                                        word: word.to_owned(),
                                        distance,
                                    })
                                    .with_meta(meta)
                                    .pipe(iter::once)
                                    .collect()
                                } else {
                                    iter::empty().collect()
                                }
                            } else {
                                iter::empty().collect()
                            }
                        }

                        "found" => {
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

                        other => Task::Command {
                            command: other.to_owned(),
                            body: components.collect::<Vec<_>>().join(" "),
                        }
                        .with_meta(meta)
                        .pipe(iter::once)
                        .collect(),
                    }
                } else if msg
                    .message_text
                    .to_lowercase()
                    .split_whitespace()
                    .any(|ea| ea == "hi")
                    && msg.message_text.to_lowercase().contains("@oxoboxowot")
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
