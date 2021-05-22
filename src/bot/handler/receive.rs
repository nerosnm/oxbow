use tap::{TapFallible, TapOptional};
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, trace};
use twitch_irc::message::ServerMessage;

use crate::msg::{BuiltInCommand, ImplicitTask, Metadata, Task, WithMeta};

pub struct ReceiveHandler {
    pub(in crate::bot) msg_rx: mpsc::UnboundedReceiver<ServerMessage>,
    pub(in crate::bot) task_tx: mpsc::UnboundedSender<(Task, Metadata)>,
    pub(in crate::bot) prefix: char,
}

impl ReceiveHandler {
    /// Loops over incoming messages and if any are a recognised command, sends
    /// a [`Task`] in `task_tx` with the appropriate task to perform.
    #[instrument(skip(self))]
    pub async fn receive(&mut self) {
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
                    let meta = Metadata {
                        id: msg.message_id.into(),
                        channel: msg.channel_login.into(),
                        sender: msg.sender.login.into(),
                    };

                    let mut components = msg.message_text.split(' ');

                    if let Some(command) =
                        components.next().and_then(|c| c.strip_prefix(self.prefix))
                    {
                        match command {
                            "command" => {
                                debug!(?meta, command = "command", "identified command");

                                if let Some(trigger) = components.next() {
                                    let response = components.collect::<Vec<_>>().join(" ");

                                    if !response.is_empty() {
                                        info!(?meta, ?trigger, ?response, "adding command");

                                        Some(
                                            Task::BuiltIn(BuiltInCommand::AddCommand {
                                                trigger: trigger.to_owned(),
                                                response: response.to_owned(),
                                            })
                                            .with_meta(meta),
                                        )
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }

                            "search" => {
                                debug!(?meta, command = "search", "identified command");

                                if &*meta.sender == "nerosnm" {
                                    Some(Task::BuiltIn(BuiltInCommand::WordSearch).with_meta(meta))
                                } else {
                                    None
                                }
                            }

                            "lower" => {
                                debug!(?meta, command = "lower", "identified command");

                                if &*meta.sender == "nerosnm" {
                                    if let Some(word) = components.next() {
                                        let distance =
                                            components.next().and_then(|d| d.parse().ok());

                                        Some(
                                            Task::BuiltIn(BuiltInCommand::WordLower {
                                                word: word.to_owned(),
                                                distance,
                                            })
                                            .with_meta(meta),
                                        )
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }

                            "upper" => {
                                debug!(?meta, command = "upper", "identified command");

                                if &*meta.sender == "nerosnm" {
                                    if let Some(word) = components.next() {
                                        let distance =
                                            components.next().and_then(|d| d.parse().ok());

                                        Some(
                                            Task::BuiltIn(BuiltInCommand::WordUpper {
                                                word: word.to_owned(),
                                                distance,
                                            })
                                            .with_meta(meta),
                                        )
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }

                            "found" => {
                                debug!(?meta, command = "found", "identified command");

                                if &*meta.sender == "nerosnm" {
                                    Some(Task::BuiltIn(BuiltInCommand::WordFound).with_meta(meta))
                                } else {
                                    None
                                }
                            }

                            other => Some(
                                Task::Command {
                                    command: other.to_owned(),
                                    body: components.collect::<Vec<_>>().join(" "),
                                }
                                .with_meta(meta),
                            ),
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

                        Some(Task::Implicit(ImplicitTask::Greet).with_meta(meta))
                    } else {
                        None
                    }
                }
                Some(ServerMessage::Notice(notice))
                    if notice
                        .message_id
                        .as_ref()
                        .map(|id| id.starts_with("msg_"))
                        .unwrap_or(false) =>
                {
                    error!(notice = %notice.message_text);
                    None
                }
                Some(msg) => {
                    debug!(?msg);
                    None
                }
                None => None,
            };

            if let Some((task, meta)) = task {
                let _ = self
                    .task_tx
                    .send(task.with_cloned_meta(&meta))
                    .tap_err(|e| error!(?meta, error = ?e, "failed to send task message"));
            }
        }
    }
}
