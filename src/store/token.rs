use async_trait::async_trait;
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use tap::TapFallible;
use thiserror::Error;
use tracing::{debug, instrument};
use twitch_irc::login::{TokenStorage, UserAccessToken};

/// Storage of a [`UserAccessToken`] in an SQLite3 database.
#[derive(Debug)]
pub struct TokenStore {
    conn_pool: Pool<SqliteConnectionManager>,
}

impl TokenStore {
    /// Create an `SQLiteStorage` with a connection to a database.
    pub fn new(conn_pool: Pool<SqliteConnectionManager>) -> Self {
        Self { conn_pool }
    }

    /// Check whether a token is currently stored in the database.
    #[instrument(skip(self))]
    pub fn has_stored_token(&self) -> Result<bool, LoadError> {
        debug!("checking for stored token");

        let conn = self.conn_pool.get()?;

        let mut stmt = conn.prepare(
            r#"
            SELECT 1
            FROM
                token
            LIMIT
                1;
            "#,
        )?;

        let mut rows = stmt.query([])?;
        let value_exists = rows.next()?.is_some();

        Ok(value_exists)
    }

    /// Store `token` in the `token` table, replacing any other values.
    #[instrument(skip(self, token))]
    pub fn store_token(&mut self, token: &UserAccessToken) -> Result<(), StoreError> {
        debug!(created_at = ?token.created_at, expires_at = ?token.expires_at, "storing token");

        // Make sure there are no other rows in the token table.
        self.conn_pool.get()?.execute(
            r#"
            DELETE FROM token;
            "#,
            params![],
        )?;

        // Insert the token into the token table.
        self.conn_pool.get()?.execute(
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
impl TokenStorage for TokenStore {
    type LoadError = LoadError;
    type UpdateError = StoreError;

    #[instrument(skip(self))]
    async fn load_token(&mut self) -> Result<UserAccessToken, Self::LoadError> {
        let conn = self.conn_pool.get()?;

        let mut stmt = conn.prepare(
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
            }).tap_ok(|t| debug!(created_at = ?t.created_at, expires_at = ?t.expires_at, "loaded stored token"))
        } else {
            Err(LoadError::NotFound)
        }
    }

    #[instrument(skip(self, token))]
    async fn update_token(&mut self, token: &UserAccessToken) -> Result<(), Self::UpdateError> {
        debug!(created_at = ?token.created_at, expires_at = ?token.expires_at, "updating stored token");

        self.conn_pool.get()?.execute(
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

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),
}

/// Errors that could arise while storing tokens in a database using
/// [`SQLiteTokenStore`].
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),
}

#[cfg(test)]
mod tests {
    use std::ops::DerefMut;

    use chrono::Duration;
    use tempfile::{tempdir, TempDir};
    use twitch_irc::login::TokenStorage;

    use super::*;

    fn storage() -> (TempDir, TokenStore) {
        let db_dir = tempdir().expect("creating a temporary directory should succeed");
        let db_path = db_dir.path().join("db.sqlite3");

        let manager = SqliteConnectionManager::file(&db_path);
        let conn_pool = Pool::new(manager).expect("creating a connection pool should succeed");

        let mut conn = conn_pool
            .get()
            .expect("getting a connection from the pool should succeed");
        crate::db::migrations::runner()
            .run(conn.deref_mut())
            .expect("running migrations should succeed");

        (db_dir, TokenStore { conn_pool })
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

    /// Test that an [`SQLiteTokenStore`] correctly reports whether a token is
    /// currently stored.
    #[tokio::test]
    async fn check_token_exists() {
        let (_db_dir, mut storage) = storage();
        let token = token_1();

        assert!(
            !storage
                .has_stored_token()
                .expect("checking for a stored token should succeed"),
            "empty storage should not report that a token is stored"
        );

        storage
            .store_token(&token)
            .expect("storing a token should succeed");

        assert!(
            storage
                .has_stored_token()
                .expect("checking for a stored token should succeed"),
            "storage containing a token should not report that it is empty"
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
