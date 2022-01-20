use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, error, info};
use twitch_irc::{login::LoginCredentials, Transport, TwitchIRCClient};

use super::Handler;
use crate::msg::{Metadata, Response};

pub struct RespondHandler<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    res_rx: broadcast::Receiver<(Response, Metadata)>,
    client: TwitchIRCClient<T, L>,
    channel: String,
}

#[async_trait]
impl<T, L> Handler for RespondHandler<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    type Input = (Response, Metadata);
    type Output = (String, String);
    type Aux = String;
    type Error = RespondError<T, L>;

    type Receiver = broadcast::Receiver<(Response, Metadata)>;
    type Sender = TwitchIRCClient<T, L>;

    async fn new(res_rx: Self::Receiver, client: Self::Sender, channel: Self::Aux) -> Self {
        debug!("starting");

        client.join(channel.clone());
        while client.get_channel_status(channel.clone()).await != (true, true) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!(?channel, "joined channel",);

        Self {
            res_rx,
            client,
            channel,
        }
    }

    fn receiver(&mut self) -> &mut Self::Receiver {
        &mut self.res_rx
    }

    fn sender(&mut self) -> &mut Self::Sender {
        &mut self.client
    }

    async fn process(
        &mut self,
        (res, meta): Self::Input,
    ) -> Result<Vec<Self::Output>, Self::Error> {
        if *meta.channel == self.channel {
            match res {
                Response::Say { message } => {
                    info!(?meta, ?message, "sending response");
                    Ok(vec![(self.channel.clone(), message)])
                }
            }
        } else {
            Ok(vec![])
        }
    }
}

#[derive(Debug, Error)]
pub enum RespondError<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    #[error("failed to receive response: {0}")]
    ReceiveResponse(#[from] broadcast::error::RecvError),

    #[error("failed to send response message: {0}")]
    Say(#[from] twitch_irc::Error<T, L>),
}
