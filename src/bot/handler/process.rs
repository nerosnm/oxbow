use std::{collections::HashMap, iter};

use async_trait::async_trait;
use chrono::Utc;
use indoc::formatdoc;
use tap::{Pipe, TapOptional};
use thiserror::Error;
use tokio::sync::{
    broadcast::{self, error::SendError},
    mpsc,
};
use tracing::{debug, error, info, instrument, warn};

use super::Handler;
use crate::{
    msg::{BuiltInCommand, Help, ImplicitTask, Metadata, Response, Task, WithMeta},
    store::{
        commands::{CommandsError, CommandsStore},
        quotes::{QuotesError, QuotesStore},
    },
    wordsearch::WordSearch,
};

pub struct ProcessHandler {
    task_rx: mpsc::UnboundedReceiver<(Task, Metadata)>,
    res_tx: broadcast::Sender<(Response, Metadata)>,
    commands: CommandsStore,
    quotes: QuotesStore,
    prefix: char,
    word_searches: HashMap<String, WordSearch>,
}

#[async_trait]
impl Handler for ProcessHandler {
    type Input = (Task, Metadata);
    type Output = (Response, Metadata);
    type Aux = (
        CommandsStore,
        QuotesStore,
        char,
        HashMap<String, WordSearch>,
    );
    type Error = ProcessError;

    type Receiver = mpsc::UnboundedReceiver<Self::Input>;
    type Sender = broadcast::Sender<(Response, Metadata)>;

    async fn new(task_rx: Self::Receiver, res_tx: Self::Sender, aux: Self::Aux) -> Self {
        let (commands, quotes, prefix, word_searches) = aux;
        Self {
            task_rx,
            res_tx,
            commands,
            quotes,
            prefix,
            word_searches,
        }
    }

    fn receiver(&mut self) -> &mut Self::Receiver {
        &mut self.task_rx
    }

    fn sender(&mut self) -> &mut Self::Sender {
        &mut self.res_tx
    }

    #[instrument(skip(self))]
    async fn process(&mut self, input: Self::Input) -> Result<Vec<Self::Output>, Self::Error> {
        let (task, meta) = input;
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
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("failed to receive task")]
    ReceiveTask,

    #[error("command error: {0}")]
    CommandError(#[from] CommandsError),

    #[error("quote error: {0}")]
    QuoteError(#[from] QuotesError),

    #[error("failed to send response: {0}")]
    SendResponse(#[from] SendError<(Response, Metadata)>),
}

impl From<()> for ProcessError {
    fn from(_: ()) -> Self {
        Self::ReceiveTask
    }
}
