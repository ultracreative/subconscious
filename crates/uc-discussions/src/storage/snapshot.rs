use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::{Storage, StorageError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerThreadRecord {
    pub thread_id: String,
    pub created_at: String,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerMessageRecord {
    pub message_id: String,
    pub thread_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub intent: String,
    pub priority: i64,
    pub state: String,
    pub delivery_receipt: Option<String>,
    pub processing_receipt: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomRecord {
    pub room_id: String,
    pub topic: String,
    pub goal: String,
    pub stage: String,
    pub active_grantee: Option<String>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub outcome_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomMemberRecord {
    pub room_id: String,
    pub member_id: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomPostRecord {
    pub room_id: String,
    pub seq: i64,
    pub post_id: String,
    pub author: String,
    pub post_type: String,
    pub content: String,
    pub reply_to_post_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomObjectionRecord {
    pub objection_id: String,
    pub room_id: String,
    pub post_id: String,
    pub author: String,
    pub reason: String,
    pub status: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomRevisionRecord {
    pub revision_id: String,
    pub room_id: String,
    pub original_post_id: String,
    pub author: String,
    pub diff_or_content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomPollRecord {
    pub poll_id: String,
    pub room_id: String,
    pub question: String,
    pub options_json: String,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomVoteRecord {
    pub poll_id: String,
    pub voter: String,
    pub vote: String,
    pub voted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomStageGrantRecord {
    pub grant_id: String,
    pub room_id: String,
    pub grantee: String,
    pub granted_by: String,
    pub granted_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CouncilRunRecord {
    pub council_id: String,
    pub name: String,
    pub question: String,
    pub intent: String,
    pub mode: String,
    pub members_json: String,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub outcome_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CouncilMemberStateRecord {
    pub council_id: String,
    pub member_name: String,
    pub status: String,
    pub response_text: Option<String>,
    pub error_text: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiscussionsSnapshot {
    pub peer_threads: Vec<PeerThreadRecord>,
    pub peer_messages: Vec<PeerMessageRecord>,
    pub rooms: Vec<RoomRecord>,
    pub room_members: Vec<RoomMemberRecord>,
    pub room_posts: Vec<RoomPostRecord>,
    pub room_objections: Vec<RoomObjectionRecord>,
    pub room_revisions: Vec<RoomRevisionRecord>,
    pub room_polls: Vec<RoomPollRecord>,
    pub room_votes: Vec<RoomVoteRecord>,
    pub room_stage_grants: Vec<RoomStageGrantRecord>,
    pub council_runs: Vec<CouncilRunRecord>,
    pub council_member_states: Vec<CouncilMemberStateRecord>,
}

impl Storage {
    pub fn export_snapshot(&self) -> Result<DiscussionsSnapshot, StorageError> {
        let connection = self.lock_connection()?;

        let mut peer_threads_stmt = connection.prepare(
            "SELECT thread_id, created_at, metadata FROM peer_threads ORDER BY thread_id ASC",
        )?;
        let peer_threads = peer_threads_stmt
            .query_map([], |row| {
                Ok(PeerThreadRecord {
                    thread_id: row.get(0)?,
                    created_at: row.get(1)?,
                    metadata: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut peer_messages_stmt = connection.prepare(
            "SELECT message_id, thread_id, from_agent, to_agent, body, intent, priority, state,
                    delivery_receipt, processing_receipt, created_at
             FROM peer_messages ORDER BY message_id ASC",
        )?;
        let peer_messages = peer_messages_stmt
            .query_map([], |row| {
                Ok(PeerMessageRecord {
                    message_id: row.get(0)?,
                    thread_id: row.get(1)?,
                    from_agent: row.get(2)?,
                    to_agent: row.get(3)?,
                    body: row.get(4)?,
                    intent: row.get(5)?,
                    priority: row.get(6)?,
                    state: row.get(7)?,
                    delivery_receipt: row.get(8)?,
                    processing_receipt: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut rooms_stmt = connection.prepare(
            "SELECT room_id, topic, goal, stage, active_grantee, status, created_at, closed_at, outcome_json
             FROM rooms ORDER BY room_id ASC",
        )?;
        let rooms = rooms_stmt
            .query_map([], |row| {
                Ok(RoomRecord {
                    room_id: row.get(0)?,
                    topic: row.get(1)?,
                    goal: row.get(2)?,
                    stage: row.get(3)?,
                    active_grantee: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                    closed_at: row.get(7)?,
                    outcome_json: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut room_members_stmt = connection.prepare(
            "SELECT room_id, member_id, role, joined_at
             FROM room_members ORDER BY room_id ASC, member_id ASC",
        )?;
        let room_members = room_members_stmt
            .query_map([], |row| {
                Ok(RoomMemberRecord {
                    room_id: row.get(0)?,
                    member_id: row.get(1)?,
                    role: row.get(2)?,
                    joined_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut room_posts_stmt = connection.prepare(
            "SELECT room_id, seq, post_id, author, post_type, content, reply_to_post_id, created_at
             FROM room_posts ORDER BY room_id ASC, seq ASC, post_id ASC",
        )?;
        let room_posts = room_posts_stmt
            .query_map([], |row| {
                Ok(RoomPostRecord {
                    room_id: row.get(0)?,
                    seq: row.get(1)?,
                    post_id: row.get(2)?,
                    author: row.get(3)?,
                    post_type: row.get(4)?,
                    content: row.get(5)?,
                    reply_to_post_id: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut room_objections_stmt = connection.prepare(
            "SELECT objection_id, room_id, post_id, author, reason, status, resolved_at
             FROM room_objections ORDER BY objection_id ASC",
        )?;
        let room_objections = room_objections_stmt
            .query_map([], |row| {
                Ok(RoomObjectionRecord {
                    objection_id: row.get(0)?,
                    room_id: row.get(1)?,
                    post_id: row.get(2)?,
                    author: row.get(3)?,
                    reason: row.get(4)?,
                    status: row.get(5)?,
                    resolved_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut room_revisions_stmt = connection.prepare(
            "SELECT revision_id, room_id, original_post_id, author, diff_or_content, created_at
             FROM room_revisions ORDER BY revision_id ASC",
        )?;
        let room_revisions = room_revisions_stmt
            .query_map([], |row| {
                Ok(RoomRevisionRecord {
                    revision_id: row.get(0)?,
                    room_id: row.get(1)?,
                    original_post_id: row.get(2)?,
                    author: row.get(3)?,
                    diff_or_content: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut room_polls_stmt = connection.prepare(
            "SELECT poll_id, room_id, question, options_json, status, created_at, closed_at
             FROM room_polls ORDER BY poll_id ASC",
        )?;
        let room_polls = room_polls_stmt
            .query_map([], |row| {
                Ok(RoomPollRecord {
                    poll_id: row.get(0)?,
                    room_id: row.get(1)?,
                    question: row.get(2)?,
                    options_json: row.get(3)?,
                    status: row.get(4)?,
                    created_at: row.get(5)?,
                    closed_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut room_votes_stmt = connection.prepare(
            "SELECT poll_id, voter, vote, voted_at
             FROM room_votes ORDER BY poll_id ASC, voter ASC",
        )?;
        let room_votes = room_votes_stmt
            .query_map([], |row| {
                Ok(RoomVoteRecord {
                    poll_id: row.get(0)?,
                    voter: row.get(1)?,
                    vote: row.get(2)?,
                    voted_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut room_stage_grants_stmt = connection.prepare(
            "SELECT grant_id, room_id, grantee, granted_by, granted_at, expires_at
             FROM room_stage_grants ORDER BY grant_id ASC",
        )?;
        let room_stage_grants = room_stage_grants_stmt
            .query_map([], |row| {
                Ok(RoomStageGrantRecord {
                    grant_id: row.get(0)?,
                    room_id: row.get(1)?,
                    grantee: row.get(2)?,
                    granted_by: row.get(3)?,
                    granted_at: row.get(4)?,
                    expires_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut council_runs_stmt = connection.prepare(
            "SELECT council_id, name, question, intent, mode, members_json, status, started_at, completed_at, outcome_json
             FROM council_runs ORDER BY council_id ASC",
        )?;
        let council_runs = council_runs_stmt
            .query_map([], |row| {
                Ok(CouncilRunRecord {
                    council_id: row.get(0)?,
                    name: row.get(1)?,
                    question: row.get(2)?,
                    intent: row.get(3)?,
                    mode: row.get(4)?,
                    members_json: row.get(5)?,
                    status: row.get(6)?,
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    outcome_json: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut council_member_states_stmt = connection.prepare(
            "SELECT council_id, member_name, status, response_text, error_text, updated_at
             FROM council_member_states ORDER BY council_id ASC, member_name ASC",
        )?;
        let council_member_states = council_member_states_stmt
            .query_map([], |row| {
                Ok(CouncilMemberStateRecord {
                    council_id: row.get(0)?,
                    member_name: row.get(1)?,
                    status: row.get(2)?,
                    response_text: row.get(3)?,
                    error_text: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DiscussionsSnapshot {
            peer_threads,
            peer_messages,
            rooms,
            room_members,
            room_posts,
            room_objections,
            room_revisions,
            room_polls,
            room_votes,
            room_stage_grants,
            council_runs,
            council_member_states,
        })
    }

    pub fn import_snapshot(&self, snapshot: &DiscussionsSnapshot) -> Result<(), StorageError> {
        let mut connection = self.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO peer_threads (thread_id, created_at, metadata)
                 VALUES (?1, ?2, ?3)",
            )?;
            for r in &snapshot.peer_threads {
                stmt.execute(params![r.thread_id, r.created_at, r.metadata])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO peer_messages (
                    message_id, thread_id, from_agent, to_agent, body, intent, priority, state,
                    delivery_receipt, processing_receipt, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for r in &snapshot.peer_messages {
                stmt.execute(params![
                    r.message_id,
                    r.thread_id,
                    r.from_agent,
                    r.to_agent,
                    r.body,
                    r.intent,
                    r.priority,
                    r.state,
                    r.delivery_receipt,
                    r.processing_receipt,
                    r.created_at
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO rooms (
                    room_id, topic, goal, stage, active_grantee, status, created_at, closed_at, outcome_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for r in &snapshot.rooms {
                stmt.execute(params![
                    r.room_id,
                    r.topic,
                    r.goal,
                    r.stage,
                    r.active_grantee,
                    r.status,
                    r.created_at,
                    r.closed_at,
                    r.outcome_json
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO room_members (room_id, member_id, role, joined_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for r in &snapshot.room_members {
                stmt.execute(params![r.room_id, r.member_id, r.role, r.joined_at])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO room_posts (
                    room_id, seq, post_id, author, post_type, content, reply_to_post_id, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for r in &snapshot.room_posts {
                stmt.execute(params![
                    r.room_id,
                    r.seq,
                    r.post_id,
                    r.author,
                    r.post_type,
                    r.content,
                    r.reply_to_post_id,
                    r.created_at
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO room_objections (
                    objection_id, room_id, post_id, author, reason, status, resolved_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for r in &snapshot.room_objections {
                stmt.execute(params![
                    r.objection_id,
                    r.room_id,
                    r.post_id,
                    r.author,
                    r.reason,
                    r.status,
                    r.resolved_at
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO room_revisions (
                    revision_id, room_id, original_post_id, author, diff_or_content, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for r in &snapshot.room_revisions {
                stmt.execute(params![
                    r.revision_id,
                    r.room_id,
                    r.original_post_id,
                    r.author,
                    r.diff_or_content,
                    r.created_at
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO room_polls (
                    poll_id, room_id, question, options_json, status, created_at, closed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for r in &snapshot.room_polls {
                stmt.execute(params![
                    r.poll_id,
                    r.room_id,
                    r.question,
                    r.options_json,
                    r.status,
                    r.created_at,
                    r.closed_at
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO room_votes (poll_id, voter, vote, voted_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for r in &snapshot.room_votes {
                stmt.execute(params![r.poll_id, r.voter, r.vote, r.voted_at])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO room_stage_grants (
                    grant_id, room_id, grantee, granted_by, granted_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for r in &snapshot.room_stage_grants {
                stmt.execute(params![
                    r.grant_id,
                    r.room_id,
                    r.grantee,
                    r.granted_by,
                    r.granted_at,
                    r.expires_at
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO council_runs (
                    council_id, name, question, intent, mode, members_json, status, started_at, completed_at, outcome_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for r in &snapshot.council_runs {
                stmt.execute(params![
                    r.council_id,
                    r.name,
                    r.question,
                    r.intent,
                    r.mode,
                    r.members_json,
                    r.status,
                    r.started_at,
                    r.completed_at,
                    r.outcome_json
                ])?;
            }
        }

        {
            let mut stmt = transaction.prepare(
                "INSERT OR REPLACE INTO council_member_states (
                    council_id, member_name, status, response_text, error_text, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for r in &snapshot.council_member_states {
                stmt.execute(params![
                    r.council_id,
                    r.member_name,
                    r.status,
                    r.response_text,
                    r.error_text,
                    r.updated_at
                ])?;
            }
        }

        transaction.commit()?;
        Ok(())
    }
}
