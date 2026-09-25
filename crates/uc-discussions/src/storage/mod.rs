mod migrations;
pub mod snapshot;

pub use snapshot::DiscussionsSnapshot;

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use chrono::{SecondsFormat, Utc};
use cortexkit_store_types::{StorageBackend, StorageDescriptor};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite storage error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("storage connection mutex is poisoned")]
    ConnectionPoisoned,
    #[error("storage descriptor backend is not SQLite")]
    UnsupportedBackend,
    #[error("lease TTL is outside the supported time range")]
    InvalidLeaseTtl,
    #[error("database schema version {found} is newer than supported version {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
}

#[derive(Clone)]
pub struct Storage {
    connection: Arc<Mutex<Connection>>,
}

impl Storage {
    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    pub fn open_path<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn from_descriptor(descriptor: &StorageDescriptor) -> Result<Self, StorageError> {
        match &descriptor.backend {
            StorageBackend::Sqlite { path } => Self::open_path(path),
            _ => Err(StorageError::UnsupportedBackend),
        }
    }

    pub fn acquire_lease(
        &self,
        resource_id: &str,
        holder_id: &str,
        ttl: Duration,
    ) -> Result<bool, StorageError> {
        let (now, expires_at) = lease_times(ttl)?;
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "INSERT INTO leases (resource_id, holder_id, acquired_at, expires_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(resource_id) DO UPDATE SET
                 holder_id = excluded.holder_id,
                 acquired_at = excluded.acquired_at,
                 expires_at = excluded.expires_at
             WHERE leases.holder_id = excluded.holder_id OR leases.expires_at <= excluded.acquired_at",
            params![resource_id, holder_id, now, expires_at],
        )?;
        transaction.commit()?;
        Ok(changed == 1)
    }

    pub fn renew_lease(
        &self,
        resource_id: &str,
        holder_id: &str,
        ttl: Duration,
    ) -> Result<bool, StorageError> {
        let (now, expires_at) = lease_times(ttl)?;
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE leases
             SET expires_at = ?1
             WHERE resource_id = ?2 AND holder_id = ?3 AND expires_at > ?4",
            params![expires_at, resource_id, holder_id, now],
        )?;
        transaction.commit()?;
        Ok(changed == 1)
    }

    pub fn release_lease(&self, resource_id: &str, holder_id: &str) -> Result<bool, StorageError> {
        let connection = self.lock_connection()?;
        let changed = connection.execute(
            "DELETE FROM leases WHERE resource_id = ?1 AND holder_id = ?2",
            params![resource_id, holder_id],
        )?;
        Ok(changed == 1)
    }

    pub fn insert_room_post(
        &self,
        room_id: &str,
        post_id: &str,
        author: &str,
        post_type: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<i64, StorageError> {
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sequence = transaction.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM room_posts WHERE room_id = ?1",
            params![room_id],
            |row| row.get::<_, i64>(0),
        )?;
        transaction.execute(
            "INSERT INTO room_posts (
                room_id, seq, post_id, author, post_type, content, reply_to_post_id, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                room_id,
                sequence,
                post_id,
                author,
                post_type,
                content,
                reply_to,
                now_timestamp()
            ],
        )?;
        transaction.commit()?;
        Ok(sequence)
    }

    pub fn record_member_state(
        &self,
        council_id: &str,
        member_name: &str,
        status: &str,
        response: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), StorageError> {
        let connection = self.lock_connection()?;
        connection.execute(
            "INSERT INTO council_member_states (
                council_id, member_name, status, response_text, error_text, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(council_id, member_name) DO UPDATE SET
                 status = excluded.status,
                 response_text = excluded.response_text,
                 error_text = excluded.error_text,
                 updated_at = excluded.updated_at",
            params![
                council_id,
                member_name,
                status,
                response,
                error,
                now_timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn all_members_terminal(
        &self,
        council_id: &str,
        declared_members: &[&str],
    ) -> Result<bool, StorageError> {
        if declared_members.is_empty() {
            return Ok(true);
        }

        let connection = self.lock_connection()?;
        let mut statement = connection.prepare(
            "SELECT status FROM council_member_states
             WHERE council_id = ?1 AND member_name = ?2",
        )?;
        for member in declared_members {
            let status = statement
                .query_row(params![council_id, member], |row| row.get::<_, String>(0))
                .optional()?;
            if !matches!(
                status.as_deref(),
                Some("completed" | "failed" | "cancelled")
            ) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, StorageError> {
        configure_connection(&connection)?;
        migrations::run(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub(crate) fn lock_connection(&self) -> Result<MutexGuard<'_, Connection>, StorageError> {
        self.connection
            .lock()
            .map_err(|_| StorageError::ConnectionPoisoned)
    }
}

fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;",
    )?;
    Ok(())
}

fn lease_times(ttl: Duration) -> Result<(String, String), StorageError> {
    let now = Utc::now();
    let ttl = chrono::Duration::from_std(ttl).map_err(|_| StorageError::InvalidLeaseTtl)?;
    let expires_at = now
        .checked_add_signed(ttl)
        .ok_or(StorageError::InvalidLeaseTtl)?;
    Ok((
        now.to_rfc3339_opts(SecondsFormat::Nanos, true),
        expires_at.to_rfc3339_opts(SecondsFormat::Nanos, true),
    ))
}

fn now_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use rusqlite::params;

    use super::{migrations, Storage};

    #[test]
    fn fresh_database_migrates_and_rerun_is_idempotent() {
        let storage = Storage::open_in_memory().expect("open storage");
        let mut connection = storage.lock_connection().expect("lock connection");
        let expected_tables = [
            "council_member_states",
            "council_runs",
            "leases",
            "peer_messages",
            "peer_threads",
            "room_members",
            "room_objections",
            "room_polls",
            "room_posts",
            "room_revisions",
            "room_stage_grants",
            "room_votes",
            "rooms",
            "schema_migrations",
        ];

        assert_eq!(
            migrations::applied_version(&connection).expect("read migration version"),
            Some(2)
        );
        let mut statement = connection
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .expect("prepare table inventory query");
        let actual_tables = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query table inventory")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect table inventory");
        drop(statement);
        assert_eq!(actual_tables, expected_tables);

        let before: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations");

        migrations::run(&mut connection).expect("rerun migrations");

        let after: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations after rerun");
        assert_eq!(before, 2);
        assert_eq!(after, before);
    }

    #[test]
    fn lease_lifecycle_enforces_holder_and_expiration() {
        let storage = Storage::open_in_memory().expect("open storage");

        assert!(storage
            .acquire_lease("room:1", "holder-a", Duration::from_millis(80))
            .expect("acquire lease"));
        assert!(!storage
            .acquire_lease("room:1", "holder-b", Duration::from_secs(1))
            .expect("reject competing holder"));
        assert!(!storage
            .renew_lease("room:1", "holder-b", Duration::from_secs(1))
            .expect("reject wrong renewal holder"));
        assert!(storage
            .renew_lease("room:1", "holder-a", Duration::from_millis(20))
            .expect("renew lease"));

        thread::sleep(Duration::from_millis(40));

        assert!(!storage
            .renew_lease("room:1", "holder-a", Duration::from_secs(1))
            .expect("reject expired renewal"));
        assert!(storage
            .acquire_lease("room:1", "holder-b", Duration::from_secs(1))
            .expect("replace expired lease"));
        assert!(!storage
            .release_lease("room:1", "holder-a")
            .expect("reject wrong release holder"));
        assert!(storage
            .release_lease("room:1", "holder-b")
            .expect("release lease"));
        assert!(storage
            .acquire_lease("room:1", "holder-a", Duration::from_secs(1))
            .expect("reacquire released lease"));
    }

    #[test]
    fn room_post_sequences_are_monotonic_per_room() {
        let storage = Storage::open_in_memory().expect("open storage");

        assert_eq!(
            storage
                .insert_room_post("room-a", "post-a1", "agent-a", "message", "one", None)
                .expect("insert first room-a post"),
            1
        );
        assert_eq!(
            storage
                .insert_room_post(
                    "room-a",
                    "post-a2",
                    "agent-b",
                    "message",
                    "two",
                    Some("post-a1"),
                )
                .expect("insert second room-a post"),
            2
        );
        assert_eq!(
            storage
                .insert_room_post("room-b", "post-b1", "agent-c", "message", "one", None)
                .expect("insert first room-b post"),
            1
        );

        let connection = storage.lock_connection().expect("lock connection");
        let rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM room_posts WHERE room_id = ?1 AND seq IN (1, 2)",
                params!["room-a"],
                |row| row.get(0),
            )
            .expect("count sequenced posts");
        assert_eq!(rows, 2);
    }

    #[test]
    fn council_terminal_check_requires_every_declared_member() {
        let storage = Storage::open_in_memory().expect("open storage");
        let members = ["alpha", "beta", "gamma"];

        assert!(!storage
            .all_members_terminal("council-1", &members)
            .expect("check missing states"));
        storage
            .record_member_state("council-1", "alpha", "completed", Some("answer"), None)
            .expect("record alpha");
        storage
            .record_member_state("council-1", "beta", "failed", None, Some("error"))
            .expect("record beta");
        storage
            .record_member_state("council-1", "gamma", "running", None, None)
            .expect("record gamma running");
        assert!(!storage
            .all_members_terminal("council-1", &members)
            .expect("check running member"));

        storage
            .record_member_state("council-1", "gamma", "cancelled", None, None)
            .expect("record gamma cancelled");
        assert!(storage
            .all_members_terminal("council-1", &members)
            .expect("check all terminal"));
        assert!(storage
            .all_members_terminal("council-1", &[])
            .expect("check empty declaration"));
    }
}
