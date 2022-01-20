use std::{error::Error, fmt::Debug};

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error};
use twitch_irc::{login::LoginCredentials, Error as IrcError, Transport, TwitchIRCClient};

mod process;
mod receive;
mod respond;

pub use process::ProcessHandler;
pub use receive::ReceiveHandler;
pub use respond::RespondHandler;

#[async_trait]
pub trait Handler {
    type Input: Send + Sync;
    type Output: Send + Sync;
    type Aux;
    type Error: Error
        + From<<Self::Receiver as Receiver<Self::Input>>::Error>
        + From<<Self::Sender as Sender<Self::Output>>::Error>
        + Send
        + Sync;

    type Receiver: Receiver<Self::Input>;
    type Sender: Sender<Self::Output>;

    async fn new(rx: Self::Receiver, tx: Self::Sender, aux: Self::Aux) -> Self;

    fn receiver(&mut self) -> &mut Self::Receiver;
    fn sender(&mut self) -> &mut Self::Sender;

    async fn run(&mut self) {
        // It seems like async_trait is causing a false-positive in the dead_code lint
        // on this function.
        #[allow(dead_code)]
        async fn run_one<H: Handler + ?Sized>(handler: &mut H) -> Result<(), H::Error> {
            let input = handler.receiver().recv().await?;
            for output in handler.process(input).await? {
                handler.sender().send(output).await?;
            }
            Ok(())
        }

        debug!("starting");

        loop {
            match run_one(self).await {
                Ok(()) => (),
                Err(err) => error!(%err),
            }
        }
    }

    async fn process(&mut self, input: Self::Input) -> Result<Vec<Self::Output>, Self::Error>;
}

#[async_trait]
pub trait Receiver<T>: Send + Sync
where
    T: Send + Sync,
{
    type Error: Debug + Send + Sync;

    async fn recv(&mut self) -> Result<T, Self::Error>;
}

#[async_trait]
impl<T> Receiver<T> for mpsc::UnboundedReceiver<T>
where
    T: Send + Sync,
{
    type Error = ();

    async fn recv(&mut self) -> Result<T, Self::Error> {
        mpsc::UnboundedReceiver::recv(self).await.ok_or(())
    }
}

#[async_trait]
impl<T> Receiver<T> for broadcast::Receiver<T>
where
    T: Clone + Send + Sync,
{
    type Error = broadcast::error::RecvError;

    async fn recv(&mut self) -> Result<T, Self::Error> {
        broadcast::Receiver::recv(self).await
    }
}

#[async_trait]
pub trait Sender<T>: Send + Sync {
    type Error: Debug + Send + Sync;

    async fn send(&mut self, v: T) -> Result<(), Self::Error>;
}

#[async_trait]
impl<T> Sender<T> for mpsc::UnboundedSender<T>
where
    T: Debug + Send + Sync,
{
    type Error = mpsc::error::SendError<T>;

    async fn send(&mut self, v: T) -> Result<(), Self::Error> {
        mpsc::UnboundedSender::send(self, v)
    }
}

#[async_trait]
impl<T> Sender<T> for broadcast::Sender<T>
where
    T: Debug + Send + Sync,
{
    type Error = broadcast::error::SendError<T>;

    async fn send(&mut self, v: T) -> Result<(), Self::Error> {
        broadcast::Sender::send(self, v).map(|_| ())
    }
}

#[async_trait]
impl<T, L> Sender<(String, String)> for TwitchIRCClient<T, L>
where
    T: Transport,
    L: LoginCredentials,
{
    type Error = IrcError<T, L>;

    async fn send(&mut self, (channel, message): (String, String)) -> Result<(), Self::Error> {
        self.say(channel, message).await
    }
}
