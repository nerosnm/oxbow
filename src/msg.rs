use std::fmt::Debug;

/// A value bundled together with some [`Metadata`].
#[derive(Debug, Clone)]
pub struct WithMeta<T>(pub T, pub Metadata)
where
    T: Debug + Clone;

/// Metadata about a task, so that it can be tracked through from the initial
/// trigger to the final response.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub id: String,
}

/// Commands to perform, which may or may not result in a [`Response`] being
/// sent.
#[derive(Debug, Clone)]
pub enum Task {
    /// Respond to an arbitrary `command` sent by a `sender` in a `channel`.
    Command {
        /// The Twitch IRC channel the command was sent in.
        channel: String,
        /// The user who sent the command.
        sender: String,
        /// The command, including its arguments, but not including the prefix.
        command: String,
    },
    Implicit(ImplicitTask),
    BuiltIn(BuiltInCommand),
}

/// Commands which are not triggered by an explicit message of the form
/// `!command` by a user, but are instead triggered in response to a heuristic.
#[derive(Debug, Clone)]
pub enum ImplicitTask {
    Greet {
        /// The Twitch IRC channel the greeting should be sent in.
        channel: String,
        /// The user to greet.
        user: String,
    },
}

/// Commands that are built in to the bot, rather than being arbitrary commands
/// stored in the database.
#[derive(Debug, Clone)]
pub enum BuiltInCommand {
    /// Add a new command to the database, with the given trigger and response.
    AddCommand {
        /// The channel the command should be added in.
        channel: String,
        /// The string after the prefix that should cause this command to run.
        trigger: String,
        /// The response that should be sent in a message.
        response: String,
    },
}

/// Commands to respond in some way to an action, such as by replying with a
/// message in an IRC channel.
#[derive(Debug, Clone)]
pub enum Response {
    /// Send a response with the text `message` in the channel `channel`.
    Say {
        /// The Twitch IRC channel the message should be sent in.
        channel: String,
        /// The message to send.
        message: String,
    },
}
