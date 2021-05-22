use std::time::Duration;

use tap::TapFallible;
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument, trace};
use twitch_irc::{login::LoginCredentials, Transport, TwitchIRCClient};

use crate::msg::{Metadata, Response};

pub struct RespondHandler<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    pub(in crate::bot) res_rx: broadcast::Receiver<(Response, Metadata)>,
    pub(in crate::bot) client: TwitchIRCClient<T, L>,
    pub(in crate::bot) channel: String,
}

impl<T, L> RespondHandler<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    /// Watches for relevant messages coming in through `msg_rx` and acts on
    /// them in `channel`, such as sending responses.
    #[instrument(skip(self))]
    pub async fn respond(&mut self) {
        debug!("starting");

        self.client.join(self.channel.clone());

        while self.client.get_channel_status(self.channel.clone()).await != (true, true) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("joined channel");

        loop {
            trace!("waiting for response message");

            let res = self
                .res_rx
                .recv()
                .await
                .tap_ok(|_| trace!("received response message"))
                .tap_err(|e| error!(error = ?e, "failed to receive response message"));

            if let Ok((response, meta)) = res {
                if *meta.channel == self.channel {
                    match response {
                        Response::Say { message } => {
                            info!(?meta, ?message, "sending response");

                            let _ = self
                                .client
                                .say(self.channel.clone(), message)
                                .await
                                .tap_err(|e| error!(?meta, error = ?e, "unable to send response"));
                        }
                    }
                }
            }
        }
    }
}
