use chrono::{Duration, Utc};
use thiserror::Error;
use tracing::{debug, error, instrument};
use twitch_api2::twitch_oauth2::{scopes::Scope, TwitchToken};
use twitch_irc::login::UserAccessToken;
use twitch_oauth2_auth_flow::AuthFlowError;

use crate::store::token::{LoadError, StoreError, TokenStore};

/// Perform the OAuth2 authentication flow with the Twitch API to get a user
/// token.
#[instrument(skip(store, client_id, client_secret))]
pub async fn authenticate(
    store: &mut TokenStore,
    client_id: &str,
    client_secret: &str,
) -> Result<(), AuthError> {
    if !store.has_stored_token()? {
        debug!("stored token not found, performing OAuth flow");

        let twitch_oauth_token = twitch_oauth2_auth_flow::auth_flow(
            client_id,
            client_secret,
            Some(vec![Scope::ChatRead, Scope::ChatEdit]),
            "http://localhost:10666",
        )?;

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

        store.store_token(&twitch_irc_token)?;
    } else {
        debug!("found stored token");
    }

    Ok(())
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
