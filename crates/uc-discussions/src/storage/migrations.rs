use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, TransactionBehavior};

#[cfg(test)]
use rusqlite::OptionalExtension;

use super::StorageError;

const LATEST_SCHEMA_VERSION: i64 = 1;

const MIGRATION_001: &str = r#"
CREATE TABLE leases (
    resource_id TEXT PRIMARY KEY,
    holder_id TEXT NOT NULL,
    acquired_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE TABLE peer_threads (
    thread_id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    metadata TEXT
);

CREATE TABLE peer_messages (
    message_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    from_agent TEXT NOT NULL,
    to_agent TEXT NOT NULL,
    body TEXT NOT NULL,
    intent TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'pending',
    delivery_receipt TEXT,
    processing_receipt TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE rooms (
    room_id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    goal TEXT NOT NULL,
    stage TEXT NOT NULL,
    active_grantee TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    closed_at TEXT,
    outcome_json TEXT
);

CREATE TABLE room_members (
    room_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    role TEXT NOT NULL,
    joined_at TEXT NOT NULL,
    PRIMARY KEY(room_id, member_id)
);

CREATE TABLE room_posts (
    room_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    post_id TEXT PRIMARY KEY,
    author TEXT NOT NULL,
    post_type TEXT NOT NULL,
    content TEXT NOT NULL,
    reply_to_post_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(room_id, seq)
);

CREATE TABLE room_objections (
    objection_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL,
    post_id TEXT NOT NULL,
    author TEXT NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    resolved_at TEXT
);

CREATE TABLE room_revisions (
    revision_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL,
    original_post_id TEXT NOT NULL,
    author TEXT NOT NULL,
    diff_or_content TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE room_polls (
    poll_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL,
    question TEXT NOT NULL,
    options_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    closed_at TEXT
);

CREATE TABLE room_votes (
    poll_id TEXT NOT NULL,
    voter TEXT NOT NULL,
    vote TEXT NOT NULL,
    voted_at TEXT NOT NULL,
    PRIMARY KEY(poll_id, voter)
);

CREATE TABLE room_stage_grants (
    grant_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL,
    grantee TEXT NOT NULL,
    granted_by TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE TABLE council_runs (
    council_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    question TEXT NOT NULL,
    intent TEXT NOT NULL,
    mode TEXT NOT NULL,
    members_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'staged',
    started_at TEXT NOT NULL,
    completed_at TEXT,
    outcome_json TEXT
);

CREATE TABLE council_member_states (
    council_id TEXT NOT NULL,
    member_name TEXT NOT NULL,
    status TEXT NOT NULL,
    response_text TEXT,
    error_text TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(council_id, member_name)
);
"#;

pub(super) fn run(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;

    let current_version = transaction
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?
        .unwrap_or(0);

    if current_version > LATEST_SCHEMA_VERSION {
        return Err(StorageError::SchemaTooNew {
            found: current_version,
            supported: LATEST_SCHEMA_VERSION,
        });
    }

    if current_version < 1 {
        transaction.execute_batch(MIGRATION_001)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![
                LATEST_SCHEMA_VERSION,
                Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
            ],
        )?;
    }

    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
pub(super) fn applied_version(connection: &Connection) -> Result<Option<i64>, StorageError> {
    connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(StorageError::from)
}
