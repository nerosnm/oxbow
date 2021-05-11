use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use thiserror::Error;

/// Storage of custom commands in an SQLite3 database.
#[derive(Debug)]
pub struct CommandsStore {
    conn_pool: Pool<SqliteConnectionManager>,
}

impl CommandsStore {
    /// Create a `CommandsStore` with a connection to a database.
    pub fn new(conn_pool: Pool<SqliteConnectionManager>) -> Self {
        Self { conn_pool }
    }

    pub fn set_command(
        &mut self,
        channel: &str,
        trigger: &str,
        response: &str,
    ) -> Result<(), CommandsError> {
        let conn = self.conn_pool.get()?;

        conn.execute(
            r#"
            INSERT INTO commands (channel, trigger, response)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(channel, trigger) DO UPDATE SET
                response = excluded.response;
            "#,
            params![channel, trigger, response],
        )?;

        Ok(())
    }

    pub fn get_command(
        &self,
        channel: &str,
        trigger: &str,
    ) -> Result<Option<String>, CommandsError> {
        let conn = self.conn_pool.get()?;

        let mut stmt = conn.prepare(
            r#"
            SELECT channel, trigger, response
            FROM commands
            WHERE channel = ?1 AND trigger = ?2
            LIMIT 1;
            "#,
        )?;

        let mut rows = stmt.query(params![channel, trigger])?;

        if let Some(row) = rows.next()? {
            row.get(2).map(Some).map_err(Into::into)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandsError {
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

    fn storage() -> (TempDir, CommandsStore) {
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

        (db_dir, CommandsStore::new(conn_pool))
    }

    #[test]
    fn set_command() {
        let (_db_dir, mut commands) = storage();

        let response = commands
            .get_command("asdf", "command")
            .expect("attempting to get the command should succeed");

        assert!(
            response.is_none(),
            "no response should be returned if the command doesn't exist"
        );

        commands
            .set_command("asdf", "command", "this is the response to the command")
            .expect("setting the command should succeed");

        let response2 = commands
            .get_command("asdf", "command")
            .expect("attempting to get the command should succeed");

        assert!(
            response2.is_some(),
            "a response should be returned if the command does exist"
        );
    }

    #[test]
    fn update_command() {
        let (_db_dir, mut commands) = storage();

        commands
            .set_command(
                "qwerty",
                "updatethis",
                "this is the response to the command",
            )
            .expect("setting the command the first time should succeed");

        commands
            .set_command("qwerty", "updatethis", "now i've changed the response")
            .expect("setting the command again should succeed in updating it");

        let response = commands
            .get_command("qwerty", "updatethis")
            .expect("attempting to get the command should succeed");

        assert_eq!(
            response.expect("response should be Some"),
            "now i've changed the response".to_owned(),
            "response should have been updated"
        );
    }
}
