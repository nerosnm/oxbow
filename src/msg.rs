/// Commands to perform, which may or may not result in a [`Response`] being
/// sent.
#[derive(Debug, Clone)]
pub enum Task {
    /// Respond to a `command` sent by a `sender` in a `channel`.
    Command {
        /// The Twitch IRC channel the command was sent in.
        channel: String,
        /// The user who sent the command.
        sender: String,
        /// The command, including its arguments, but not including the prefix.
        command: String,
    },
    Implicit(ImplicitTask),
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
