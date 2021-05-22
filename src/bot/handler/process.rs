use std::collections::HashMap;

use tap::{Pipe, TapFallible, TapOptional};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::{
    commands::CommandsStore,
    msg::{BuiltInCommand, ImplicitTask, Metadata, Response, Task, WithMeta},
    wordsearch::WordSearch,
};

pub struct ProcessHandler {
    pub(in crate::bot) task_rx: mpsc::UnboundedReceiver<(Task, Metadata)>,
    pub(in crate::bot) res_tx: broadcast::Sender<(Response, Metadata)>,
    pub(in crate::bot) commands: CommandsStore,
    pub(in crate::bot) prefix: char,
    pub(in crate::bot) word_searches: HashMap<String, WordSearch>,
}

impl ProcessHandler {
    /// Loops over incoming [`Task`]s, acts on them, and if necessary, sends a
    /// [`Response`] in `res_tx` with the appropriate response to send.
    #[instrument(skip(self))]
    pub async fn process(&mut self) {
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
                Some((Task::Command { command, body }, meta)) => {
                    info!(?meta, ?command, ?body);

                    self.commands
                        .get_command(&meta.channel, &command)
                        .expect("getting a command should succeed")
                        .map(|message| Response::Say { message })
                        .tap_none(|| warn!(?meta, ?command, "command not found"))
                        .unwrap_or_else(|| Response::Say {
                            message: format!("Command {}{} not found", self.prefix, command),
                        })
                        .with_meta(meta)
                        .pipe(Some)
                }
                Some((Task::Implicit(ImplicitTask::Greet), meta)) => {
                    info!(?meta, "implicit greet task");

                    Some(
                        Response::Say {
                            message: format!("uwu *nuzzles @{}*", meta.sender),
                        }
                        .with_meta(meta),
                    )
                }
                Some((Task::BuiltIn(BuiltInCommand::AddCommand { trigger, response }), meta)) => {
                    info!(?meta, ?trigger, ?response, "add command task");

                    let already_exists = self
                        .commands
                        .get_command(&meta.channel, &trigger)
                        .expect("getting a command should succeed")
                        .is_some();

                    self.commands
                        .set_command(&meta.channel, &trigger, &response)
                        .expect("setting a command should succeed");

                    let verb = if already_exists { "Updated" } else { "Added" };

                    Some(
                        Response::Say {
                            message: format!("{} {}{}", verb, self.prefix, trigger),
                        }
                        .with_meta(meta),
                    )
                }
                Some((Task::BuiltIn(BuiltInCommand::WordSearch), meta)) => {
                    info!(?meta, "word search task");

                    let word_search = self
                        .word_searches
                        .entry(meta.channel.to_string())
                        .and_modify(|ws| ws.reset())
                        .or_default();

                    Some(
                        Response::Say {
                            message: format!("!wg {}", word_search.guess()),
                        }
                        .with_meta(meta),
                    )
                }
                Some((Task::BuiltIn(BuiltInCommand::WordLower { word, distance }), meta)) => {
                    info!(?meta, ?word, "word lower task");

                    if let Some(word_search) = self.word_searches.get_mut(&*meta.channel) {
                        word_search.set_lower(&word, distance);

                        Some(
                            Response::Say {
                                message: format!("!wg {}", word_search.guess()),
                            }
                            .with_meta(meta),
                        )
                    } else {
                        Some(
                            Response::Say {
                                message: format!(
                                    "No word search in progress! Start one with {}search",
                                    self.prefix
                                ),
                            }
                            .with_meta(meta),
                        )
                    }
                }
                Some((Task::BuiltIn(BuiltInCommand::WordUpper { word, distance }), meta)) => {
                    info!(?meta, ?word, "word upper task");

                    if let Some(word_search) = self.word_searches.get_mut(&*meta.channel) {
                        word_search.set_upper(&word, distance);

                        Some(
                            Response::Say {
                                message: format!("!wg {}", word_search.guess()),
                            }
                            .with_meta(meta),
                        )
                    } else {
                        Some(
                            Response::Say {
                                message: format!(
                                    "No word search in progress! Start one with {}search",
                                    self.prefix
                                ),
                            }
                            .with_meta(meta),
                        )
                    }
                }
                Some((Task::BuiltIn(BuiltInCommand::WordFound), meta)) => {
                    info!(?meta, "word found task");

                    if let Some(word_search) = self.word_searches.get_mut(&*meta.channel) {
                        word_search.reset();

                        Some(
                            Response::Say {
                                message: "Word search stopped".to_owned(),
                            }
                            .with_meta(meta),
                        )
                    } else {
                        Some(
                            Response::Say {
                                message: "No word search in progress!".to_owned(),
                            }
                            .with_meta(meta),
                        )
                    }
                }
                None => None,
            };

            if let Some((res, meta)) = response {
                let _ = self
                    .res_tx
                    .send(res.with_cloned_meta(&meta))
                    .tap_err(|e| error!(?meta, error = ?e, "failed to send response message"));
            }
        }
    }
}
