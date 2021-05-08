use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use thiserror::Error;
use twitch_api2::twitch_oauth2::{scopes::Scope, TwitchToken};
use twitch_irc::login::{TokenStorage, UserAccessToken};

/// Perform the OAuth2 authentication flow with the Twitch API to get a user
/// token.
pub async fn authenticate(
    store: &mut SQLiteTokenStore,
    client_id: &str,
    client_secret: &str,
) -> Result<UserAccessToken, AuthError> {
    match store.load_token().await {
        Ok(token) => Ok(token),
        Err(LoadError::NotFound) => {
            let twitch_oauth_token = twitch_oauth2_auth_flow::auth_flow(
                client_id,
                client_secret,
                Some(vec![Scope::ChatRead, Scope::ChatEdit]),
                "http://localhost:10666",
            )
            .expect("authentication should succeed");

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

            Ok(twitch_irc_token)
        }
        Err(err) => Err(err.into()),
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("error loading token: {0}")]
    Load(#[from] LoadError),

    #[error("error storing token: {0}")]
    Store(#[from] StoreError),
}

/// Storage of a [`UserAccessToken`] in an SQLite3 database.
#[derive(Debug)]
pub struct SQLiteTokenStore {
    conn: Connection,
}

impl SQLiteTokenStore {
    /// Create an `SQLiteStorage` with a connection to a database.
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Store `token` in the `token` table, replacing any other values.
    pub fn store_token(&mut self, token: &UserAccessToken) -> Result<(), StoreError> {
        // Make sure there are no other rows in the token table.
        self.conn.execute(
            r#"
            DELETE FROM token;
            "#,
            params![],
        )?;

        // Insert the token into the token table.
        self.conn.execute(
            r#"
            INSERT INTO token (
                access_token,
                refresh_token,
                created_at,
                expires_at
            )
            VALUES (?1, ?2, ?3, ?4);
            "#,
            params![
                token.access_token,
                token.refresh_token,
                token.created_at.to_rfc3339(),
                token.expires_at.map(|ex| ex.to_rfc3339()),
            ],
        )?;

        Ok(())
    }
}

#[async_trait]
impl TokenStorage for SQLiteTokenStore {
    type LoadError = LoadError;
    type UpdateError = StoreError;

    async fn load_token(&mut self) -> Result<UserAccessToken, Self::LoadError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                access_token,
                refresh_token,
                created_at,
                expires_at
            FROM
                token
            LIMIT
                1;
            "#,
        )?;

        let mut rows = stmt.query([])?;

        if let Some(token) = rows.next()? {
            let access_token = token.get::<_, String>(0)?;
            let refresh_token = token.get::<_, String>(1)?;
            let created_at_str = token.get::<_, String>(2)?;
            let expires_at_str = token.get::<_, Option<String>>(3)?;

            let created_at = created_at_str.parse::<DateTime<Utc>>()?;
            let expires_at = expires_at_str
                .map(|ea| ea.parse::<DateTime<Utc>>())
                .transpose()?;

            Ok(UserAccessToken {
                access_token,
                refresh_token,
                created_at,
                expires_at,
            })
        } else {
            Err(LoadError::NotFound)
        }
    }

    async fn update_token(&mut self, token: &UserAccessToken) -> Result<(), Self::UpdateError> {
        self.conn.execute(
            r#"
            UPDATE token
            SET
                access_token = ?1,
                refresh_token = ?2,
                created_at = ?3,
                expires_at = ?4;
            "#,
            params![
                token.access_token,
                token.refresh_token,
                token.created_at.to_rfc3339(),
                token.expires_at.map(|ex| ex.to_rfc3339()),
            ],
        )?;

        Ok(())
    }
}

/// Errors that could arise while loading stored tokens from a database using
/// [`SQLiteTokenStore`].
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("no stored token found")]
    NotFound,

    #[error("error parsing a date/time: {0}")]
    Parse(#[from] chrono::format::ParseError),

    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
}

/// Errors that could arise while storing tokens in a database using
/// [`SQLiteTokenStore`].
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use tempfile::{tempdir, TempDir};
    use twitch_irc::login::TokenStorage;

    use super::*;

    fn storage() -> (TempDir, SQLiteTokenStore) {
        let db_dir = tempdir().expect("creating a temporary directory should succeed");
        let db_path = db_dir.path().join("db.sqlite3");

        let mut conn = Connection::open(&db_path)
            .expect("opening a database at a tempfile path should succeed");

        crate::db::migrations::runner()
            .run(&mut conn)
            .expect("running migrations should succeed");

        (db_dir, SQLiteTokenStore { conn })
    }

    fn token_1() -> UserAccessToken {
        UserAccessToken {
            access_token: "test access token".into(),
            refresh_token: "test refresh token".into(),
            created_at: Utc::now() - Duration::hours(1),
            expires_at: Some(Utc::now() + Duration::hours(3)),
        }
    }

    fn token_2() -> UserAccessToken {
        UserAccessToken {
            access_token: "updated access token".into(),
            refresh_token: "updated refresh token".into(),
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    /// Test that storing an initial token in an [`SQLiteTokenStore`] succeeds
    /// and stores a correct value that can be loaded again accurately.
    #[tokio::test]
    async fn initial_store_token() {
        let (_db_dir, mut storage) = storage();
        let token = token_1();

        storage
            .store_token(&token)
            .expect("storing the initial token should succeed");

        let loaded = storage
            .load_token()
            .await
            .expect("loading the token again should succeed");

        assert_eq!(
            token.access_token, loaded.access_token,
            "loaded access token does not match the original"
        );

        assert_eq!(
            token.refresh_token, loaded.refresh_token,
            "loaded refresh token does not match the original"
        );

        assert_eq!(
            token.created_at, loaded.created_at,
            "loaded created_at does not match the original"
        );

        assert_eq!(
            token.expires_at, loaded.expires_at,
            "loaded expires_at does not match the original"
        );
    }

    /// Test that updating a stored token in an [`SQLiteTokenStore`] succeeds
    /// and all of the values are correctly changed to their new values.
    #[tokio::test]
    async fn update_token() {
        let (_db_dir, mut storage) = storage();
        let old_token = token_1();
        let new_token = token_2();

        storage
            .store_token(&old_token)
            .expect("storing the initial token should succeed");

        let loaded = storage
            .load_token()
            .await
            .expect("loading the old token should succeed");

        assert_eq!(
            old_token.access_token, loaded.access_token,
            "loaded access token does not match the old token"
        );

        assert_eq!(
            old_token.refresh_token, loaded.refresh_token,
            "loaded refresh token does not match the old token"
        );

        assert_eq!(
            old_token.created_at, loaded.created_at,
            "loaded created_at does not match the old token"
        );

        assert_eq!(
            old_token.expires_at, loaded.expires_at,
            "loaded expires_at does not match the old token"
        );

        storage
            .update_token(&new_token)
            .await
            .expect("updating the token with a new value should succeed");

        let loaded = storage
            .load_token()
            .await
            .expect("loading the new token should succeed");

        assert_eq!(
            new_token.access_token, loaded.access_token,
            "loaded access token does not match the new token"
        );

        assert_eq!(
            new_token.refresh_token, loaded.refresh_token,
            "loaded refresh token does not match the new token"
        );

        assert_eq!(
            new_token.created_at, loaded.created_at,
            "loaded created_at does not match the new token"
        );

        assert_eq!(
            new_token.expires_at, loaded.expires_at,
            "loaded expires_at does not match the new token"
        );
    }
}
