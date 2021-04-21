use twitch_api2::twitch_oauth2::{scopes::Scope, UserToken};

/// Perform the OAuth2 authentication flow with the Twitch API to get a user
/// token.
pub async fn authenticate(client_id: &str, client_secret: &str) -> UserToken {
    twitch_oauth2_auth_flow::auth_flow(
        client_id,
        client_secret,
        Some(vec![Scope::ChatRead, Scope::ChatEdit]),
        "http://localhost:10666",
    )
    .expect("authentication should succeed")
}
