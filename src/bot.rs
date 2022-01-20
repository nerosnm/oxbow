use std::collections::HashMap;

use chrono::{Duration, Utc};
use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tap::TapFallible;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, instrument};
use twitch_api2::{helix::Scope, twitch_oauth2::TwitchToken};
use twitch_irc::{
    login::{RefreshingLoginCredentials, UserAccessToken},
    ClientConfig, TCPTransport, TwitchIRCClient,
};
use twitch_oauth2_auth_flow::AuthFlowError;

use crate::{
    parse::oxbow::CommandParser,
    store::{
        commands::CommandsStore,
        quotes::QuotesStore,
        token::{LoadError, StoreError, TokenStore},
    },
};

mod builder;
mod handler;

pub use self::{
    builder::{BotBuilder, BotTheBuilder},
    handler::{ProcessHandler, ReceiveHandler, RespondHandler},
};

/// A `Bot` contains all the authentication keys and configuration values necessary to run a Twitch
/// bot, but has not yet communicated with Twitch.
///
/// To authenticate with the Twitch API so that the bot can be run, call [`Bot::authenticate()`] to
/// produce an [`AuthenticatedBot`].
pub struct Bot {
    client_id: String,
    client_secret: String,
    twitch_name: String,
    channels: Vec<String>,
    prefix: char,
    conn_pool: Pool<SqliteConnectionManager>,
}

impl Bot {
    /// Create a [`BotBuilder`] to build an instance of [`Bot`].
    pub fn builder() -> BotBuilder {
        BotBuilder::default()
    }

    /// Create [`BotTheBuilder`] to build an instance of [`Bot`].
    pub fn the_builder() -> BotTheBuilder {
        BotTheBuilder::default()
    }

    /// Authenticate using the OAuth authorization code flow, to allow the bot to communicate in
    /// Twitch IRC channels.
    #[instrument(skip(self))]
    pub fn authenticate(self) -> Result<AuthenticatedBot, AuthError> {
        let Bot {
            client_id,
            client_secret,
            twitch_name,
            channels,
            prefix,
            conn_pool,
        } = self;

        // Create a token store with a connection to the database, so that we can access and update
        // stored tokens.
        let mut token_store = TokenStore::new(conn_pool.clone());

        // If we don't have a token pair (access token + refresh token) stored already, we'll need
        // to get a new one.
        if !token_store.has_stored_token()? {
            debug!("stored token not found, performing OAuth flow");

            let twitch_oauth_token = twitch_oauth2_auth_flow::auth_flow(
                &client_id,
                &client_secret,
                Some(vec![Scope::ChatRead, Scope::ChatEdit]),
                "http://localhost:10666",
            )
            .tap_ok(|_| info!("successfully performed auth flow to obtain token"))
            .tap_err(|_| error!("failed to perform auth flow to obtain token"))?;

            let twitch_irc_token = UserAccessToken {
                access_token: twitch_oauth_token.access_token.secret().to_owned(),
                refresh_token: twitch_oauth_token
                    .refresh_token
                    .as_ref()
                    .expect("refresh token should be provided")
                    .secret()
                    .to_owned(),
                created_at: Utc::now(),
                expires_at: Some(
                    Utc::now()
                        + Duration::from_std(twitch_oauth_token.expires_in())
                            .expect("duration should convert from std to chrono"),
                ),
            };

            token_store.store_token(&twitch_irc_token)?;
        } else {
            info!("found stored token");
        }

        Ok(AuthenticatedBot {
            twitch_name,
            client_id,
            client_secret,
            token_store,
            channels,
            prefix,
            conn_pool,
        })
    }
}

/// Errors that could arise while performing authentication with Twitch.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("error loading token: {0}")]
    Load(#[from] LoadError),

    #[error("error storing token: {0}")]
    Store(#[from] StoreError),

    #[error("auth flow error: {0}")]
    AuthFlow(#[from] AuthFlowError),
}

/// An `AuthenticatedBot` is a bot that has authenticated with the Twitch API and has the necessary
/// token stored in order to communicate through Twitch chat.
///
/// Created through [`Bot::authenticate()`].
pub struct AuthenticatedBot {
    twitch_name: String,
    client_id: String,
    client_secret: String,
    token_store: TokenStore,
    channels: Vec<String>,
    prefix: char,
    conn_pool: Pool<SqliteConnectionManager>,
}

impl AuthenticatedBot {
    /// Main run loop for the bot.
    ///
    /// Spawns tasks to receive messages, and to send messages to each connected
    /// channel.
    #[instrument(skip(self), fields(channels = ?self.channels, twitch_name = %self.twitch_name, prefix = %self.prefix))]
    pub async fn run(&mut self) -> Result<(), BotError> {
        info!("starting bot");

        let credentials = RefreshingLoginCredentials::new(
            self.twitch_name.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
            self.token_store.clone(),
        );
        let config = ClientConfig::new_simple(credentials);
        let (msg_rx, client) = TwitchIRCClient::<TCPTransport, _>::new(config);

        // Channel for the receive loop to trigger tasks in the process loop.
        let (task_tx, task_rx) = mpsc::unbounded_channel();

        // Channel for the process loop to trigger responses in the response
        // loops.
        let (res_tx_orig, _) = broadcast::channel(16);

        // Spawn a receive loop to interpret incoming messages and turn them
        // into Tasks if necessary.
        let prefix = self.prefix;
        let twitch_name = self.twitch_name.clone();
        let receive_loop = tokio::spawn(async move {
            let mut handler = ReceiveHandler {
                msg_rx,
                task_tx,
                prefix,
                twitch_name,
                parser: CommandParser::new(),
            };

            handler.receive_loop().await;
        });

        // Spawn a processing loop to interpret Tasks and turn them into
        // Responses if necessary.
        let res_tx = res_tx_orig.clone();
        let commands = CommandsStore::new(self.conn_pool.clone());
        let quotes = QuotesStore::new(self.conn_pool.clone());
        let prefix = self.prefix;
        let process_loop = tokio::spawn(async move {
            let mut handler = ProcessHandler {
                task_rx,
                res_tx,
                commands,
                quotes,
                prefix,
                word_searches: HashMap::new(),
            };

            handler.process_loop().await;
        });

        // For every channel, we need a response loop to perform Responses if
        // they're relevant to that channel.
        for channel in self.channels.iter() {
            info!(?channel, "joining channel");

            let res_rx = res_tx_orig.subscribe();
            let client = client.clone();
            let channel = channel.to_owned();

            tokio::spawn(async move {
                let mut handler = RespondHandler {
                    res_rx,
                    client,
                    channel,
                };

                handler.respond_loop().await;
            });
        }

        receive_loop.await.unwrap();
        process_loop.await.unwrap();

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum BotError {
    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),
}
