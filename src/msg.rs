use std::{fmt::Debug, sync::Arc};

/// Metadata about a task (data that is common to all tasks and helps identify
/// it through its whole lifecycle).
#[derive(Debug, Clone)]
pub struct Metadata {
    /// The ID of the original message that caused the command.
    pub id: Arc<str>,
    /// The channel the command was sent in.
    pub channel: Arc<str>,
    /// The user who sent the command.
    pub sender: Arc<str>,
}

pub trait WithMeta<M> {
    fn with_meta(self, meta: M) -> (Self, M)
    where
        Self: Sized,
    {
        (self, meta)
    }

    fn with_cloned_meta(self, meta: &M) -> (Self, M)
    where
        Self: Sized,
        M: Clone,
    {
        self.with_meta(meta.clone())
    }
}

pub enum Location {
    Twitch { channel: String },
}

/// Tasks to perform, which may or may not result in a [`Response`] being sent.
#[derive(Debug, Clone)]
pub enum Task {
    /// Respond to an arbitrary `command`, identified from a message beginning
    /// with a preset prefix.
    Command {
        /// The command, not including the prefix.
        command: String,
    },
    Implicit(ImplicitTask),
    BuiltIn(BuiltInCommand),
    Help(Help),
}

impl WithMeta<Metadata> for Task {}

/// Commands which are not triggered by an explicit message of the form
/// `!command` by a user, but are instead triggered in response to a heuristic.
#[derive(Debug, Clone)]
pub enum ImplicitTask {
    Greet,
}

/// Commands that are built in to the bot, rather than being arbitrary commands
/// stored in the database.
#[derive(Debug, Clone)]
pub enum BuiltInCommand {
    /// Add a new command to the database, with the given trigger and response.
    AddCommand {
        /// The string after the prefix that should cause this command to run.
        trigger: String,
        /// The response that should be sent in a message.
        response: String,
    },
    /// Add a new quote to the database.
    AddQuote {
        /// The username of the user being quoted.
        username: String,
        /// The key, if provided, for retrieval of this quote.
        key: Option<String>,
        /// The quote itself.
        text: String,
    },
    /// Get a quote by its key.
    GetQuote {
        /// The key of the quote to get.
        key: String,
    },
    /// Get a random quote.
    RandomQuote,
    /// Start a word search run.
    WordSearch,
    /// Set the lower bound after a guess.
    WordLower {
        word: String,
        distance: Option<usize>,
    },
    /// Set the upper bound after a guess.
    WordUpper {
        word: String,
        distance: Option<usize>,
    },
    /// End a word search run.
    WordFound,
}

#[derive(Debug, Clone)]
pub enum Help {
    /// Respond with general help text.
    General,
    /// Respond with help text for the quote command.
    Quote,
}

/// Commands to respond in some way to an action, such as by replying with a
/// message in an IRC channel.
#[derive(Debug, Clone)]
pub enum Response {
    /// Send a response with the text `message`.
    Say {
        /// The message to send.
        message: String,
    },
}

impl WithMeta<Metadata> for Response {}
