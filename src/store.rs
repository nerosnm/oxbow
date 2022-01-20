//! Persistent storage of data, including custom commands, quotes, and
//! authentication tokens.

use refinery::embed_migrations;
use rusqlite::Connection;
use thiserror::Error;

pub mod commands;
pub mod quotes;
pub mod token;

// Embeds migrations from the `migrations/` folder at the root of the crate.
embed_migrations!();

/// Open a connection to the in-memory SQLite3 database.
pub fn in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;

    Ok(conn)
}

/// Errors that could be encountered while performing database operations.
#[derive(Debug, Error)]
pub enum Error {
    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
