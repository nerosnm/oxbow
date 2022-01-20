use std::{collections::HashMap, iter};

use chrono::Utc;
use indoc::formatdoc;
use tap::{Pipe, TapFallible, TapOptional};
use thiserror::Error;
use tokio::sync::{
    broadcast::{self, error::SendError},
    mpsc,
};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::{
    msg::{BuiltInCommand, Help, ImplicitTask, Metadata, Response, Task, WithMeta},
    store::{
        commands::{CommandsError, CommandsStore},
        quotes::{QuotesError, QuotesStore},
    },
    wordsearch::WordSearch,
};

pub struct ProcessHandler {
    pub(in crate::bot) task_rx: mpsc::UnboundedReceiver<(Task, Metadata)>,
    pub(in crate::bot) res_tx: broadcast::Sender<(Response, Metadata)>,
    pub(in crate::bot) commands: CommandsStore,
    pub(in crate::bot) quotes: QuotesStore,
    pub(in crate::bot) prefix: char,
    pub(in crate::bot) word_searches: HashMap<String, WordSearch>,
}

impl ProcessHandler {
    /// Loops over incoming [`Task`]s, acts on them, and if necessary, sends
    /// [`Response`]s in `res_tx`.
    #[instrument(skip(self))]
    pub async fn process_loop(&mut self) {
        debug!("starting");

        loop {
            match self.process().await {
                Ok(()) => {}
                Err(err) => error!(%err),
            }
        }
    }

    /// Gets an incoming [`Task`], acts on it, and if necessary, sends
    /// [`Response`]s in `res_tx`.
    #[instrument(skip(self))]
    async fn process(&mut self) -> Result<(), ProcessError> {
        trace!("waiting for task message");

        let (task, meta) = self.task_rx.recv().await.ok_or(ProcessError::ReceiveTask)?;

        trace!("received task message");

        for (response, meta) in self.handle_task(task, meta).await? {
            self.send_response(response, meta).await?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_task(
        &mut self,
        task: Task,
        meta: Metadata,
    ) -> Result<Vec<(Response, Metadata)>, ProcessError> {
        let responses = match task {
            Task::Command { command } => {
                info!(?meta, ?command, "user-defined command task");

                self.commands
                    .get_command(&meta.channel, &command)?
                    .tap_none(|| warn!(?meta, ?command, "command not found"))
                    .map(|message| vec![Response::Say { message }])
                    .unwrap_or_default()
                    .into_iter()
                    .map(|ea| ea.with_cloned_meta(&meta))
                    .collect()
            }
            Task::Implicit(ImplicitTask::Greet) => {
                info!(?meta, "implicit greet task");

                Response::Say {
                    message: format!("uwu *nuzzles @{}*", meta.sender),
                }
                .with_meta(meta)
                .pipe(iter::once)
                .collect()
            }
            Task::BuiltIn(BuiltInCommand::AddCommand { trigger, response }) => {
                info!(?meta, ?trigger, ?response, "add command task");

                let already_exists = self
                    .commands
                    .get_command(&meta.channel, &trigger)?
                    .is_some();

                self.commands
                    .set_command(&meta.channel, &trigger, &response)?;

                let verb = if already_exists { "Updated" } else { "Added" };

                Response::Say {
                    message: format!("{} {}{}", verb, self.prefix, trigger),
                }
                .with_meta(meta)
                .pipe(iter::once)
                .collect()
            }
            Task::Help(Help::General) => {
                info!(?meta, "general help task");

                Response::Say {
                    message: formatdoc!(
                        "
                        @{sender} See oxbow.cacti.dev/commands for help
                        ",
                        sender = meta.sender,
                    ),
                }
                .with_meta(meta)
                .pipe(iter::once)
                .collect()
            }
            Task::Help(Help::Quote) => {
                info!(?meta, "quote help task");

                Response::Say {
                    message: formatdoc!(
                        "
                        @{sender} See oxbow.cacti.dev/commands#quotes for help with {prefix}quote
                        ",
                        sender = meta.sender,
                        prefix = self.prefix,
                    ),
                }
                .with_meta(meta)
                .pipe(iter::once)
                .collect()
            }
            Task::BuiltIn(BuiltInCommand::AddQuote {
                username,
                key,
                text,
            }) => {
                info!(?meta, ?username, ?key, ?text, "add quote task");

                let when = Utc::now();
                let date_str = when.format("%d %b %Y");
                let time_str = when.format("%H:%M");

                if let Some(key) = key {
                    self.quotes
                        .add_quote_keyed(&meta.channel, &username, &key, &text, when)?;

                    Response::Say {
                        message: format!(
                            "Quote #{key} added from @{username} on {date_str} at {time_str} UTC",
                        ),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                } else {
                    self.quotes
                        .add_quote_unkeyed(&meta.channel, &username, &text, when)?;

                    Response::Say {
                        message: format!(
                            "Quote added from @{username} on {date_str} at {time_str} UTC",
                        ),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                }
            }
            Task::BuiltIn(BuiltInCommand::GetQuote { key }) => {
                info!(?meta, ?key, "get quote by key task");

                if let Some(quote) = self.quotes.get_quote_keyed(&meta.channel, &key)? {
                    Response::Say {
                        message: format!("{}", quote),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                } else {
                    iter::empty().collect()
                }
            }
            Task::BuiltIn(BuiltInCommand::RandomQuote) => {
                info!(?meta, "get random quote task");

                if let Some(quote) = self.quotes.get_quote_random(&meta.channel)? {
                    Response::Say {
                        message: format!("{}", quote),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                } else {
                    iter::empty().collect()
                }
            }
            Task::BuiltIn(BuiltInCommand::WordSearch) => {
                info!(?meta, "word search task");

                let word_search = self
                    .word_searches
                    .entry(meta.channel.to_string())
                    .and_modify(|ws| ws.reset())
                    .or_default();

                Response::Say {
                    message: format!("!wg {}", word_search.guess()),
                }
                .with_meta(meta)
                .pipe(iter::once)
                .collect()
            }
            Task::BuiltIn(BuiltInCommand::WordLower { word, distance }) => {
                info!(?meta, ?word, "word lower task");

                if let Some(word_search) = self.word_searches.get_mut(&*meta.channel) {
                    word_search.set_lower(&word, distance);

                    Response::Say {
                        message: format!("!wg {}", word_search.guess()),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                } else {
                    Response::Say {
                        message: format!(
                            "No word search in progress! Start one with {}search",
                            self.prefix
                        ),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                }
            }
            Task::BuiltIn(BuiltInCommand::WordUpper { word, distance }) => {
                info!(?meta, ?word, "word upper task");

                if let Some(word_search) = self.word_searches.get_mut(&*meta.channel) {
                    word_search.set_upper(&word, distance);

                    Response::Say {
                        message: format!("!wg {}", word_search.guess()),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                } else {
                    Response::Say {
                        message: format!(
                            "No word search in progress! Start one with {}search",
                            self.prefix
                        ),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                }
            }
            Task::BuiltIn(BuiltInCommand::WordFound) => {
                info!(?meta, "word found task");

                if let Some(word_search) = self.word_searches.get_mut(&*meta.channel) {
                    word_search.reset();

                    Response::Say {
                        message: "Word search stopped".to_owned(),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                } else {
                    Response::Say {
                        message: "No word search in progress!".to_owned(),
                    }
                    .with_meta(meta)
                    .pipe(iter::once)
                    .collect()
                }
            }
        };

        debug!(?responses, "returning responses");

        Ok(responses)
    }

    #[instrument(skip(self))]
    async fn send_response(&self, response: Response, meta: Metadata) -> Result<(), ProcessError> {
        debug!(?meta, ?response, "sending response");

        let _ = self
            .res_tx
            .send(response.with_cloned_meta(&meta))
            .tap_err(|e| error!(?meta, error = ?e, "failed to send response message"))?;

        Ok(())
    }
}

#[derive(Debug, Error)]
enum ProcessError {
    #[error("failed to receive task")]
    ReceiveTask,

    #[error("command error: {0}")]
    CommandError(#[from] CommandsError),

    #[error("quote error: {0}")]
    QuoteError(#[from] QuotesError),

    #[error("failed to send response: {0}")]
    SendResponse(#[from] SendError<(Response, Metadata)>),
}
