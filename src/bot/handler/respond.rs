use std::time::Duration;

use thiserror::Error;
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
    /// Loops over incoming [`Response`]s and acts on them, such as by sending
    /// messages in a channel.
    #[instrument(skip(self))]
    pub async fn respond_loop(&mut self) {
        debug!("starting");

        self.client.join(self.channel.clone());

        while self.client.get_channel_status(self.channel.clone()).await != (true, true) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("joined channel");

        loop {
            match self.respond().await {
                Ok(()) => {}
                Err(err) => error!(%err),
            }
        }
    }

    /// Gets an incoming [`Response`] and acts on it, such as by sending a
    /// message in a channel.
    #[instrument(skip(self))]
    async fn respond(&mut self) -> Result<(), RespondError<T, L>> {
        trace!("waiting for response message");

        let (res, meta) = self.res_rx.recv().await?;

        if *meta.channel == self.channel {
            self.send_response(res, meta).await?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn send_response(
        &mut self,
        res: Response,
        meta: Metadata,
    ) -> Result<(), RespondError<T, L>> {
        match res {
            Response::Say { message } => {
                info!(?meta, ?message, "sending response");

                self.client.say(self.channel.clone(), message).await?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
enum RespondError<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    #[error("failed to receive response: {0}")]
    ReceiveResponse(#[from] broadcast::error::RecvError),

    #[error("failed to send response message: {0}")]
    Say(#[from] twitch_irc::Error<T, L>),
}
