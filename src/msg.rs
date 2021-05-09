/// Messages, to be sent through [broadcast channels](`tokio::sync::broadcast`),
/// indicating that an action should be taken.
#[derive(Debug, Clone)]
pub enum Message {
    /// Send a response with the text `message` in the channel `channel`.
    Response { channel: String, message: String },
}
