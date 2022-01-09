use std::fmt;

use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rand::prelude::IteratorRandom;
use rusqlite::{
    ffi::{Error as SqliteFfiError, ErrorCode},
    params, Error as SqliteError,
};
use tap::Pipe;
use thiserror::Error;

pub struct Quote {
    pub quote: String,
    pub username: String,
    pub when: Option<DateTime<Utc>>,
    pub key: Option<String>,
}

impl fmt::Display for Quote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\" - @{}", self.quote, self.username)?;

        if let Some(when) = self.when {
            write!(f, ", {}", when.format("%d %b %Y"))?;
        }

        if let Some(key) = &self.key {
            write!(f, " (#{})", key)?;
        }

        Ok(())
    }
}

/// Storage of custom commands in an SQLite3 database.
#[derive(Debug, Clone)]
pub struct QuotesStore {
    conn_pool: Pool<SqliteConnectionManager>,
}

impl QuotesStore {
    /// Create a `QuotesStore` with a connection to a database.
    pub fn new(conn_pool: Pool<SqliteConnectionManager>) -> Self {
        Self { conn_pool }
    }

    pub fn add_quote_unkeyed(
        &self,
        channel: &str,
        username: &str,
        text: &str,
        time: DateTime<Utc>,
    ) -> Result<(), QuotesError> {
        let conn = self.conn_pool.get()?;

        match conn.execute(
            r#"
            INSERT OR ROLLBACK INTO quotes (channel, username, quote, time)
            VALUES (?1, ?2, ?3, ?4);
            "#,
            params![channel, username, text, time],
        ) {
            Ok(_) => Ok(()),
            Err(SqliteError::SqliteFailure(
                SqliteFfiError {
                    code: ErrorCode::ConstraintViolation,
                    ..
                },
                _,
            )) => Err(QuotesError::DuplicateQuote {
                channel: channel.into(),
                username: username.into(),
                text: text.into(),
            }),
            Err(err) => Err(err.into()),
        }
    }

    pub fn add_quote_keyed(
        &self,
        channel: &str,
        username: &str,
        key: &str,
        text: &str,
        time: DateTime<Utc>,
    ) -> Result<(), QuotesError> {
        let conn = self.conn_pool.get()?;

        match conn.execute(
            r#"
            INSERT OR ROLLBACK INTO quotes (channel, username, key, quote, time)
            VALUES (?1, ?2, ?3, ?4, ?5);
            "#,
            params![channel, username, key, text, time],
        ) {
            Ok(_) => Ok(()),
            Err(SqliteError::SqliteFailure(
                SqliteFfiError {
                    code: ErrorCode::ConstraintViolation,
                    ..
                },
                _,
            )) => {
                // If we've failed due to a constraint violation here, it could either be because
                // the key is already used for another quote or because the text of the quote
                // already exists for this user. We'll run a query for any quotes with the provided
                // key to determine which one it is.

                let mut key_stmt = conn.prepare(
                    r#"
                    SELECT key
                    FROM quotes
                    WHERE key = ?1
                    LIMIT 1;
                    "#,
                )?;

                let mut same_key = key_stmt.query(params![key])?;

                if same_key.next()?.is_some() {
                    QuotesError::DuplicateKey {
                        channel: channel.into(),
                        key: key.into(),
                    }
                } else {
                    QuotesError::DuplicateQuote {
                        channel: channel.into(),
                        username: username.into(),
                        text: text.into(),
                    }
                }
                .pipe(Err)
            }
            Err(err) => Err(err.into()),
        }
    }

    pub fn get_quote_keyed(&self, channel: &str, key: &str) -> Result<Option<Quote>, QuotesError> {
        let conn = self.conn_pool.get()?;

        let mut stmt = conn.prepare(
            r#"
            SELECT channel, quote, username, time, key
            FROM quotes
            WHERE channel = ?1 AND key = ?2
            LIMIT 1;
            "#,
        )?;

        let mut rows = stmt.query(params![channel, key])?;

        if let Some(row) = rows.next()? {
            Quote {
                quote: row.get(1)?,
                username: row.get(2)?,
                when: row.get(3)?,
                key: row.get(4)?,
            }
            .pipe(Some)
            .pipe(Ok)
        } else {
            Ok(None)
        }
    }

    pub fn get_quote_random(&self, channel: &str) -> Result<Option<Quote>, QuotesError> {
        let conn = self.conn_pool.get()?;

        let mut stmt = conn.prepare(
            r#"
            SELECT channel, quote, username, time, key
            FROM quotes
            WHERE channel = ?1;
            "#,
        )?;

        let all = stmt
            .query_map(params![channel], |row| {
                Quote {
                    quote: row.get(1)?,
                    username: row.get(2)?,
                    when: row.get(3)?,
                    key: row.get(4)?,
                }
                .pipe(Ok)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        all.into_iter().choose(&mut rand::thread_rng()).pipe(Ok)
    }
}

#[derive(Debug, Error)]
pub enum QuotesError {
    #[error("duplicate quote from @{username} in channel {channel}: {text}")]
    DuplicateQuote {
        channel: String,
        username: String,
        text: String,
    },

    #[error("duplicate quote key #{key} in channel {channel}")]
    DuplicateKey { channel: String, key: String },

    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("r2d2 error: {0}")]
    R2d2(#[from] r2d2::Error),
}

#[cfg(test)]
mod tests {
    use std::ops::DerefMut;

    use tempfile::{tempdir, TempDir};

    use super::*;

    fn storage() -> (TempDir, QuotesStore) {
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

        (db_dir, QuotesStore::new(conn_pool))
    }

    // #[test]
    // fn set_command() {
    //     let (_db_dir, commands) = storage();
    //
    //     let response = commands
    //         .get_command("asdf", "command")
    //         .expect("attempting to get the command should succeed");
    //
    //     assert!(
    //         response.is_none(),
    //         "no response should be returned if the command doesn't exist"
    //     );
    //
    //     commands
    //         .set_command("asdf", "command", "this is the response to the command")
    //         .expect("setting the command should succeed");
    //
    //     let response2 = commands
    //         .get_command("asdf", "command")
    //         .expect("attempting to get the command should succeed");
    //
    //     assert!(
    //         response2.is_some(),
    //         "a response should be returned if the command does exist"
    //     );
    // }
    //
    // #[test]
    // fn update_command() {
    //     let (_db_dir, commands) = storage();
    //
    //     commands
    //         .set_command(
    //             "qwerty",
    //             "updatethis",
    //             "this is the response to the command",
    //         )
    //         .expect("setting the command the first time should succeed");
    //
    //     commands
    //         .set_command("qwerty", "updatethis", "now i've changed the response")
    //         .expect("setting the command again should succeed in updating it");
    //
    //     let response = commands
    //         .get_command("qwerty", "updatethis")
    //         .expect("attempting to get the command should succeed");
    //
    //     assert_eq!(
    //         response.expect("response should be Some"),
    //         "now i've changed the response".to_owned(),
    //         "response should have been updated"
    //     );
    // }
}
