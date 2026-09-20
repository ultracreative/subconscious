#![forbid(unsafe_code)]

use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    protocol::rooms::{
        CloseRoomRequest, CloseRoomResponse, CreatePollRequest, CreatePollResponse,
        CreateRoomRequest, CreateRoomResponse, GetRoomRequest, GetRoomResponse,
        GrantStageRequest, GrantStageResponse, JoinRoomRequest, JoinRoomResponse,
        ObjectRoomRequest, ObjectRoomResponse, PostRoomRequest, PostRoomResponse,
        ReviseRoomRequest, ReviseRoomResponse, RoomMemberDto, RoomObjectionDto, RoomPollDto,
        RoomPostDto, RoomRevisionDto, RoomVoteDto, VotePollRequest, VotePollResponse,
    },
    Storage,
};

use super::ServiceError;

#[derive(Clone)]
pub struct RoomsService {
    storage: Storage,
}

impl RoomsService {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn create_room(
        &self,
        req: CreateRoomRequest,
    ) -> Result<CreateRoomResponse, ServiceError> {
        require_non_empty("topic", &req.topic)?;
        require_non_empty("goal", &req.goal)?;
        require_non_empty("stage", &req.stage)?;
        require_non_empty("creator", &req.creator)?;

        let room_id = format!("room-{}", Uuid::new_v4());
        let created_at = now_timestamp();
        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO rooms (room_id, topic, goal, stage, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5)",
            params![room_id, req.topic, req.goal, req.stage, created_at],
        )?;
        transaction.execute(
            "INSERT INTO room_members (room_id, member_id, role, joined_at)
             VALUES (?1, ?2, 'creator', ?3)",
            params![room_id, req.creator, created_at],
        )?;
        transaction.commit()?;

        Ok(CreateRoomResponse {
            room_id,
            status: "active".to_owned(),
            created_at,
        })
    }

    pub fn join_room(&self, req: JoinRoomRequest) -> Result<JoinRoomResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        require_non_empty("member_id", &req.member_id)?;
        let role = if req.role.trim().is_empty() {
            "participant".to_owned()
        } else {
            req.role
        };
        let joined_at = now_timestamp();

        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_active_room(&transaction, &req.room_id)?;
        transaction.execute(
            "INSERT INTO room_members (room_id, member_id, role, joined_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![req.room_id, req.member_id, role, joined_at],
        )?;
        transaction.commit()?;

        Ok(JoinRoomResponse {
            ok: true,
            room_id: req.room_id,
            member: RoomMemberDto {
                member_id: req.member_id,
                role,
                joined_at,
            },
        })
    }

    pub fn post(&self, req: PostRoomRequest) -> Result<PostRoomResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        require_non_empty("author", &req.author)?;
        require_non_empty("post_type", &req.post_type)?;
        require_non_empty("content", &req.content)?;
        if let Some(reply_to_post_id) = req.reply_to_post_id.as_deref() {
            require_non_empty("reply_to_post_id", reply_to_post_id)?;
        }

        {
            let connection = self.storage.lock_connection()?;
            require_active_room(&connection, &req.room_id)?;
            if let Some(reply_to_post_id) = req.reply_to_post_id.as_deref() {
                require_post_in_room(&connection, &req.room_id, reply_to_post_id)?;
            }
        }

        let post_id = format!("post-{}", Uuid::new_v4());
        let seq = self.storage.insert_room_post(
            &req.room_id,
            &post_id,
            &req.author,
            &req.post_type,
            &req.content,
            req.reply_to_post_id.as_deref(),
        )?;
        let connection = self.storage.lock_connection()?;
        let post = connection.query_row(
            "SELECT seq, post_id, author, post_type, content, reply_to_post_id, created_at
             FROM room_posts WHERE post_id = ?1",
            params![post_id],
            room_post_from_row,
        )?;

        Ok(PostRoomResponse { post, seq })
    }

    pub fn object(&self, req: ObjectRoomRequest) -> Result<ObjectRoomResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        require_non_empty("post_id", &req.post_id)?;
        require_non_empty("author", &req.author)?;
        require_non_empty("reason", &req.reason)?;

        let objection_id = format!("obj-{}", Uuid::new_v4());
        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_room(&transaction, &req.room_id)?;
        require_post_in_room(&transaction, &req.room_id, &req.post_id)?;
        transaction.execute(
            "INSERT INTO room_objections (
                objection_id, room_id, post_id, author, reason, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'open')",
            params![objection_id, req.room_id, req.post_id, req.author, req.reason],
        )?;
        transaction.commit()?;

        Ok(ObjectRoomResponse {
            objection: RoomObjectionDto {
                objection_id,
                post_id: req.post_id,
                author: req.author,
                reason: req.reason,
                status: "open".to_owned(),
                resolved_at: None,
            },
        })
    }

    pub fn revise(&self, req: ReviseRoomRequest) -> Result<ReviseRoomResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        require_non_empty("original_post_id", &req.original_post_id)?;
        require_non_empty("author", &req.author)?;
        require_non_empty("diff_or_content", &req.diff_or_content)?;

        let revision_id = format!("rev-{}", Uuid::new_v4());
        let created_at = now_timestamp();
        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_room(&transaction, &req.room_id)?;
        require_post_in_room(&transaction, &req.room_id, &req.original_post_id)?;
        transaction.execute(
            "INSERT INTO room_revisions (
                revision_id, room_id, original_post_id, author, diff_or_content, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                revision_id,
                req.room_id,
                req.original_post_id,
                req.author,
                req.diff_or_content,
                created_at
            ],
        )?;
        transaction.commit()?;

        Ok(ReviseRoomResponse {
            revision: RoomRevisionDto {
                revision_id,
                original_post_id: req.original_post_id,
                author: req.author,
                diff_or_content: req.diff_or_content,
                created_at,
            },
        })
    }

    pub fn create_poll(
        &self,
        req: CreatePollRequest,
    ) -> Result<CreatePollResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        require_non_empty("question", &req.question)?;
        require_non_empty("created_by", &req.created_by)?;
        if req.options.is_empty() {
            return Err(ServiceError::InvalidRequest(
                "options must not be empty".to_owned(),
            ));
        }
        for option in &req.options {
            require_non_empty("poll option", option)?;
        }

        let poll_id = format!("poll-{}", Uuid::new_v4());
        let created_at = now_timestamp();
        let options_json = serde_json::to_string(&req.options)
            .map_err(|error| invalid_json("poll options", error))?;
        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_active_room(&transaction, &req.room_id)?;
        transaction.execute(
            "INSERT INTO room_polls (
                poll_id, room_id, question, options_json, status, created_at
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5)",
            params![poll_id, req.room_id, req.question, options_json, created_at],
        )?;
        transaction.commit()?;

        Ok(CreatePollResponse {
            poll: RoomPollDto {
                poll_id,
                question: req.question,
                options: req.options,
                votes: Vec::new(),
                status: "active".to_owned(),
                created_at,
                closed_at: None,
            },
        })
    }

    pub fn vote_poll(&self, req: VotePollRequest) -> Result<VotePollResponse, ServiceError> {
        require_non_empty("poll_id", &req.poll_id)?;
        require_non_empty("voter", &req.voter)?;
        require_non_empty("vote", &req.vote)?;

        let voted_at = now_timestamp();
        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let status = transaction
            .query_row(
                "SELECT status FROM room_polls WHERE poll_id = ?1",
                params![req.poll_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| ServiceError::NotFound(format!("poll {}", req.poll_id)))?;
        if status != "active" {
            return Err(ServiceError::InvalidRequest(format!(
                "poll {} is not active",
                req.poll_id
            )));
        }
        transaction.execute(
            "INSERT OR REPLACE INTO room_votes (poll_id, voter, vote, voted_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![req.poll_id, req.voter, req.vote, voted_at],
        )?;
        transaction.commit()?;

        Ok(VotePollResponse {
            ok: true,
            poll_id: req.poll_id,
            voter: req.voter,
            vote: req.vote,
        })
    }

    pub fn grant_stage(
        &self,
        req: GrantStageRequest,
    ) -> Result<GrantStageResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        require_non_empty("grantee", &req.grantee)?;
        require_non_empty("granted_by", &req.granted_by)?;

        let granted_at = Utc::now();
        let ttl = chrono::Duration::from_std(Duration::from_millis(req.ttl_ms)).map_err(|_| {
            ServiceError::InvalidRequest("ttl_ms is outside the supported range".to_owned())
        })?;
        let expires_at = granted_at
            .checked_add_signed(ttl)
            .ok_or_else(|| {
                ServiceError::InvalidRequest("ttl_ms is outside the supported range".to_owned())
            })?
            .to_rfc3339_opts(SecondsFormat::Nanos, true);
        let granted_at = granted_at.to_rfc3339_opts(SecondsFormat::Nanos, true);
        let grant_id = format!("grant-{}", Uuid::new_v4());

        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_room(&transaction, &req.room_id)?;
        transaction.execute(
            "INSERT INTO room_stage_grants (
                grant_id, room_id, grantee, granted_by, granted_at, expires_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                grant_id,
                req.room_id,
                req.grantee,
                req.granted_by,
                granted_at,
                expires_at
            ],
        )?;
        transaction.execute(
            "UPDATE rooms SET active_grantee = ?1 WHERE room_id = ?2",
            params![req.grantee, req.room_id],
        )?;
        transaction.commit()?;

        Ok(GrantStageResponse {
            grant_id,
            room_id: req.room_id,
            grantee: req.grantee,
            expires_at,
        })
    }

    pub fn close_room(
        &self,
        req: CloseRoomRequest,
    ) -> Result<CloseRoomResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        let outcome = RoomOutcome {
            decisions: req.decisions,
            dissent: req.dissent,
            outstanding_actions: req.outstanding_actions,
        };
        let outcome_json = serde_json::to_string(&outcome)
            .map_err(|error| invalid_json("room outcome", error))?;
        let closed_at = now_timestamp();

        let mut connection = self.storage.lock_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_room(&transaction, &req.room_id)?;
        transaction.execute(
            "UPDATE rooms
             SET status = 'closed', closed_at = ?1, outcome_json = ?2
             WHERE room_id = ?3",
            params![closed_at, outcome_json, req.room_id],
        )?;
        transaction.commit()?;

        Ok(CloseRoomResponse {
            room_id: req.room_id,
            status: "closed".to_owned(),
            closed_at,
        })
    }

    pub fn get_room(&self, req: GetRoomRequest) -> Result<GetRoomResponse, ServiceError> {
        require_non_empty("room_id", &req.room_id)?;
        let connection = self.storage.lock_connection()?;
        let room = connection
            .query_row(
                "SELECT room_id, topic, goal, stage, active_grantee, status, created_at,
                        closed_at, outcome_json
                 FROM rooms WHERE room_id = ?1",
                params![req.room_id],
                |row| {
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
                },
            )
            .optional()?
            .ok_or_else(|| ServiceError::NotFound(format!("room {}", req.room_id)))?;

        let members = query_members(&connection, &room.room_id)?;
        let posts = query_posts(&connection, &room.room_id)?;
        let objections = query_objections(&connection, &room.room_id)?;
        let revisions = query_revisions(&connection, &room.room_id)?;
        let polls = query_polls(&connection, &room.room_id)?;
        let outcome = match room.outcome_json {
            Some(outcome_json) => serde_json::from_str(&outcome_json)
                .map_err(|error| invalid_json("stored room outcome", error))?,
            None => RoomOutcome::default(),
        };

        Ok(GetRoomResponse {
            room_id: room.room_id,
            topic: room.topic,
            goal: room.goal,
            stage: room.stage,
            active_grantee: room.active_grantee,
            status: room.status,
            created_at: room.created_at,
            closed_at: room.closed_at,
            decisions: outcome.decisions,
            dissent: outcome.dissent,
            outstanding_actions: outcome.outstanding_actions,
            members,
            posts,
            objections,
            revisions,
            polls,
        })
    }
}

#[derive(Default, Deserialize, Serialize)]
struct RoomOutcome {
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    dissent: Vec<String>,
    #[serde(default)]
    outstanding_actions: Vec<String>,
}

struct RoomRecord {
    room_id: String,
    topic: String,
    goal: String,
    stage: String,
    active_grantee: Option<String>,
    status: String,
    created_at: String,
    closed_at: Option<String>,
    outcome_json: Option<String>,
}

fn require_room(connection: &Connection, room_id: &str) -> Result<String, ServiceError> {
    connection
        .query_row(
            "SELECT status FROM rooms WHERE room_id = ?1",
            params![room_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| ServiceError::NotFound(format!("room {room_id}")))
}

fn require_active_room(connection: &Connection, room_id: &str) -> Result<(), ServiceError> {
    let status = require_room(connection, room_id)?;
    if status != "active" {
        return Err(ServiceError::InvalidRequest(format!(
            "room {room_id} is not active"
        )));
    }
    Ok(())
}

fn require_post_in_room(
    connection: &Connection,
    room_id: &str,
    post_id: &str,
) -> Result<(), ServiceError> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM room_posts WHERE room_id = ?1 AND post_id = ?2",
            params![room_id, post_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Err(ServiceError::NotFound(format!(
            "post {post_id} in room {room_id}"
        )));
    }
    Ok(())
}

fn query_members(
    connection: &Connection,
    room_id: &str,
) -> Result<Vec<RoomMemberDto>, ServiceError> {
    let mut statement = connection.prepare(
        "SELECT member_id, role, joined_at FROM room_members
         WHERE room_id = ?1 ORDER BY joined_at ASC, member_id ASC",
    )?;
    let members = statement
        .query_map(params![room_id], |row| {
            Ok(RoomMemberDto {
                member_id: row.get(0)?,
                role: row.get(1)?,
                joined_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ServiceError::from)?;
    Ok(members)
}

fn query_posts(
    connection: &Connection,
    room_id: &str,
) -> Result<Vec<RoomPostDto>, ServiceError> {
    let mut statement = connection.prepare(
        "SELECT seq, post_id, author, post_type, content, reply_to_post_id, created_at
         FROM room_posts WHERE room_id = ?1 ORDER BY seq ASC",
    )?;
    let posts = statement
        .query_map(params![room_id], room_post_from_row)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ServiceError::from)?;
    Ok(posts)
}

fn query_objections(
    connection: &Connection,
    room_id: &str,
) -> Result<Vec<RoomObjectionDto>, ServiceError> {
    let mut statement = connection.prepare(
        "SELECT objection_id, post_id, author, reason, status, resolved_at
         FROM room_objections WHERE room_id = ?1 ORDER BY rowid ASC",
    )?;
    let objections = statement
        .query_map(params![room_id], |row| {
            Ok(RoomObjectionDto {
                objection_id: row.get(0)?,
                post_id: row.get(1)?,
                author: row.get(2)?,
                reason: row.get(3)?,
                status: row.get(4)?,
                resolved_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ServiceError::from)?;
    Ok(objections)
}

fn query_revisions(
    connection: &Connection,
    room_id: &str,
) -> Result<Vec<RoomRevisionDto>, ServiceError> {
    let mut statement = connection.prepare(
        "SELECT revision_id, original_post_id, author, diff_or_content, created_at
         FROM room_revisions WHERE room_id = ?1 ORDER BY created_at ASC, revision_id ASC",
    )?;
    let revisions = statement
        .query_map(params![room_id], |row| {
            Ok(RoomRevisionDto {
                revision_id: row.get(0)?,
                original_post_id: row.get(1)?,
                author: row.get(2)?,
                diff_or_content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ServiceError::from)?;
    Ok(revisions)
}

fn query_polls(
    connection: &Connection,
    room_id: &str,
) -> Result<Vec<RoomPollDto>, ServiceError> {
    let mut statement = connection.prepare(
        "SELECT poll_id, question, options_json, status, created_at, closed_at
         FROM room_polls WHERE room_id = ?1 ORDER BY created_at ASC, poll_id ASC",
    )?;
    let poll_rows = statement
        .query_map(params![room_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    poll_rows
        .into_iter()
        .map(
            |(poll_id, question, options_json, status, created_at, closed_at)| {
                let options = serde_json::from_str(&options_json)
                    .map_err(|error| invalid_json("stored poll options", error))?;
                let votes = query_votes(connection, &poll_id)?;
                Ok(RoomPollDto {
                    poll_id,
                    question,
                    options,
                    votes,
                    status,
                    created_at,
                    closed_at,
                })
            },
        )
        .collect()
}

fn query_votes(
    connection: &Connection,
    poll_id: &str,
) -> Result<Vec<RoomVoteDto>, ServiceError> {
    let mut statement = connection.prepare(
        "SELECT voter, vote, voted_at FROM room_votes
         WHERE poll_id = ?1 ORDER BY voted_at ASC, voter ASC",
    )?;
    let votes = statement
        .query_map(params![poll_id], |row| {
            Ok(RoomVoteDto {
                voter: row.get(0)?,
                vote: row.get(1)?,
                voted_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ServiceError::from)?;
    Ok(votes)
}

fn room_post_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoomPostDto> {
    Ok(RoomPostDto {
        seq: row.get(0)?,
        post_id: row.get(1)?,
        author: row.get(2)?,
        post_type: row.get(3)?,
        content: row.get(4)?,
        reply_to_post_id: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn require_non_empty(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(ServiceError::InvalidRequest(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn invalid_json(context: &str, error: serde_json::Error) -> ServiceError {
    ServiceError::InvalidRequest(format!("{context} JSON is invalid: {error}"))
}

fn now_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> RoomsService {
        RoomsService::new(Storage::open_in_memory().expect("open in-memory storage"))
    }

    fn create_room(service: &RoomsService) -> CreateRoomResponse {
        service
            .create_room(CreateRoomRequest {
                topic: "Release readiness".to_owned(),
                goal: "Reach an attributable decision".to_owned(),
                stage: "deliberation".to_owned(),
                creator: "alice".to_owned(),
            })
            .expect("create room")
    }

    fn post(service: &RoomsService, room_id: &str, author: &str, content: &str) -> RoomPostDto {
        service
            .post(PostRoomRequest {
                room_id: room_id.to_owned(),
                author: author.to_owned(),
                post_type: "argument".to_owned(),
                content: content.to_owned(),
                reply_to_post_id: None,
            })
            .expect("post to room")
            .post
    }

    #[test]
    fn room_creation_and_joining_persist_membership() {
        let service = service();
        let created = create_room(&service);

        assert!(created.room_id.starts_with("room-"));
        assert!(Uuid::parse_str(created.room_id.trim_start_matches("room-")).is_ok());
        assert_eq!(created.status, "active");

        let joined = service
            .join_room(JoinRoomRequest {
                room_id: created.room_id.clone(),
                member_id: "bob".to_owned(),
                role: String::new(),
            })
            .expect("join room");
        assert!(joined.ok);
        assert_eq!(joined.member.member_id, "bob");
        assert_eq!(joined.member.role, "participant");

        let room = service
            .get_room(GetRoomRequest {
                room_id: created.room_id,
            })
            .expect("get room");
        assert_eq!(room.members.len(), 2);
        assert!(room
            .members
            .iter()
            .any(|member| member.member_id == "alice" && member.role == "creator"));
        assert!(room
            .members
            .iter()
            .any(|member| member.member_id == "bob" && member.role == "participant"));
    }

    #[test]
    fn post_sequences_are_monotonic_across_authors() {
        let service = service();
        let room = create_room(&service);

        let first = post(&service, &room.room_id, "alice", "ship now");
        let second = post(&service, &room.room_id, "bob", "wait for metrics");
        let third = post(&service, &room.room_id, "carol", "ship behind a flag");

        assert_eq!([first.seq, second.seq, third.seq], [1, 2, 3]);
        let fetched = service
            .get_room(GetRoomRequest {
                room_id: room.room_id,
            })
            .expect("get room");
        assert_eq!(
            fetched.posts.iter().map(|post| post.seq).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            fetched
                .posts
                .iter()
                .map(|post| post.author.as_str())
                .collect::<Vec<_>>(),
            vec!["alice", "bob", "carol"]
        );
    }

    #[test]
    fn objections_and_revisions_target_posts_in_the_room() {
        let service = service();
        let room = create_room(&service);
        let original = post(&service, &room.room_id, "alice", "deploy globally");

        let objection = service
            .object(ObjectRoomRequest {
                room_id: room.room_id.clone(),
                post_id: original.post_id.clone(),
                author: "bob".to_owned(),
                reason: "No rollback evidence".to_owned(),
            })
            .expect("raise objection")
            .objection;
        assert!(objection.objection_id.starts_with("obj-"));
        assert_eq!(objection.post_id, original.post_id);
        assert_eq!(objection.status, "open");

        let revision = service
            .revise(ReviseRoomRequest {
                room_id: room.room_id.clone(),
                original_post_id: original.post_id.clone(),
                author: "alice".to_owned(),
                diff_or_content: "deploy to 10% with automatic rollback".to_owned(),
            })
            .expect("submit revision")
            .revision;
        assert!(revision.revision_id.starts_with("rev-"));
        assert_eq!(revision.original_post_id, original.post_id);

        let fetched = service
            .get_room(GetRoomRequest {
                room_id: room.room_id,
            })
            .expect("get room");
        assert_eq!(fetched.objections, vec![objection]);
        assert_eq!(fetched.revisions, vec![revision]);
    }

    #[test]
    fn polling_voting_and_stage_grants_are_visible_on_the_room() {
        let service = service();
        let room = create_room(&service);
        let poll = service
            .create_poll(CreatePollRequest {
                room_id: room.room_id.clone(),
                question: "Approve canary release?".to_owned(),
                options: vec!["yes".to_owned(), "no".to_owned()],
                created_by: "alice".to_owned(),
            })
            .expect("create poll")
            .poll;
        assert!(poll.poll_id.starts_with("poll-"));
        assert!(poll.votes.is_empty());

        service
            .vote_poll(VotePollRequest {
                poll_id: poll.poll_id.clone(),
                voter: "bob".to_owned(),
                vote: "yes".to_owned(),
            })
            .expect("vote yes");
        service
            .vote_poll(VotePollRequest {
                poll_id: poll.poll_id.clone(),
                voter: "bob".to_owned(),
                vote: "no".to_owned(),
            })
            .expect("replace vote");

        let granted = service
            .grant_stage(GrantStageRequest {
                room_id: room.room_id.clone(),
                grantee: "bob".to_owned(),
                granted_by: "alice".to_owned(),
                ttl_ms: 60_000,
            })
            .expect("grant stage");
        assert!(granted.grant_id.starts_with("grant-"));
        assert_eq!(granted.grantee, "bob");
        assert!(chrono::DateTime::parse_from_rfc3339(&granted.expires_at).is_ok());

        let fetched = service
            .get_room(GetRoomRequest {
                room_id: room.room_id,
            })
            .expect("get room");
        assert_eq!(fetched.active_grantee.as_deref(), Some("bob"));
        assert_eq!(fetched.polls.len(), 1);
        assert_eq!(fetched.polls[0].poll_id, poll.poll_id);
        assert_eq!(fetched.polls[0].votes.len(), 1);
        assert_eq!(fetched.polls[0].votes[0].voter, "bob");
        assert_eq!(fetched.polls[0].votes[0].vote, "no");
    }

    #[test]
    fn closure_and_get_room_preserve_the_complete_timeline_and_outcome() {
        let service = service();
        let room = create_room(&service);
        service
            .join_room(JoinRoomRequest {
                room_id: room.room_id.clone(),
                member_id: "bob".to_owned(),
                role: "reviewer".to_owned(),
            })
            .expect("join reviewer");
        let first = post(&service, &room.room_id, "alice", "release with a canary");
        let second = post(&service, &room.room_id, "bob", "require rollback metrics");
        let objection = service
            .object(ObjectRoomRequest {
                room_id: room.room_id.clone(),
                post_id: first.post_id.clone(),
                author: "bob".to_owned(),
                reason: "Canary threshold is unspecified".to_owned(),
            })
            .expect("object")
            .objection;
        let revision = service
            .revise(ReviseRoomRequest {
                room_id: room.room_id.clone(),
                original_post_id: first.post_id.clone(),
                author: "alice".to_owned(),
                diff_or_content: "release to 10%; rollback above 1% errors".to_owned(),
            })
            .expect("revise")
            .revision;
        let poll = service
            .create_poll(CreatePollRequest {
                room_id: room.room_id.clone(),
                question: "Accept revised plan?".to_owned(),
                options: vec!["accept".to_owned(), "reject".to_owned()],
                created_by: "alice".to_owned(),
            })
            .expect("create poll")
            .poll;
        service
            .vote_poll(VotePollRequest {
                poll_id: poll.poll_id.clone(),
                voter: "bob".to_owned(),
                vote: "accept".to_owned(),
            })
            .expect("vote");
        service
            .grant_stage(GrantStageRequest {
                room_id: room.room_id.clone(),
                grantee: "bob".to_owned(),
                granted_by: "alice".to_owned(),
                ttl_ms: 1_000,
            })
            .expect("grant stage");

        let decisions = vec!["alice: ship the 10% canary".to_owned()];
        let dissent = vec!["bob: preferred a 5% canary".to_owned()];
        let actions = vec!["bob: publish rollback metrics".to_owned()];
        let closed = service
            .close_room(CloseRoomRequest {
                room_id: room.room_id.clone(),
                decisions: decisions.clone(),
                dissent: dissent.clone(),
                outstanding_actions: actions.clone(),
            })
            .expect("close room");
        assert_eq!(closed.status, "closed");

        let fetched = service
            .get_room(GetRoomRequest {
                room_id: room.room_id,
            })
            .expect("get closed room");
        assert_eq!(fetched.topic, "Release readiness");
        assert_eq!(fetched.goal, "Reach an attributable decision");
        assert_eq!(fetched.stage, "deliberation");
        assert_eq!(fetched.status, "closed");
        assert_eq!(fetched.closed_at.as_deref(), Some(closed.closed_at.as_str()));
        assert_eq!(fetched.decisions, decisions);
        assert_eq!(fetched.dissent, dissent);
        assert_eq!(fetched.outstanding_actions, actions);
        assert_eq!(fetched.members.len(), 2);
        assert_eq!(fetched.posts, vec![first, second]);
        assert_eq!(fetched.objections, vec![objection]);
        assert_eq!(fetched.revisions, vec![revision]);
        assert_eq!(fetched.polls.len(), 1);
        assert_eq!(fetched.polls[0].poll_id, poll.poll_id);
        assert_eq!(fetched.polls[0].votes.len(), 1);
        assert_eq!(fetched.polls[0].votes[0].vote, "accept");
        assert_eq!(fetched.active_grantee.as_deref(), Some("bob"));
    }

    #[test]
    fn closed_rooms_reject_members_posts_and_new_polls() {
        let service = service();
        let room = create_room(&service);
        service
            .close_room(CloseRoomRequest {
                room_id: room.room_id.clone(),
                decisions: Vec::new(),
                dissent: Vec::new(),
                outstanding_actions: Vec::new(),
            })
            .expect("close room");

        assert!(matches!(
            service.join_room(JoinRoomRequest {
                room_id: room.room_id.clone(),
                member_id: "bob".to_owned(),
                role: "participant".to_owned(),
            }),
            Err(ServiceError::InvalidRequest(_))
        ));
        assert!(matches!(
            service.post(PostRoomRequest {
                room_id: room.room_id.clone(),
                author: "alice".to_owned(),
                post_type: "argument".to_owned(),
                content: "too late".to_owned(),
                reply_to_post_id: None,
            }),
            Err(ServiceError::InvalidRequest(_))
        ));
        assert!(matches!(
            service.create_poll(CreatePollRequest {
                room_id: room.room_id,
                question: "Reopen?".to_owned(),
                options: vec!["yes".to_owned(), "no".to_owned()],
                created_by: "alice".to_owned(),
            }),
            Err(ServiceError::InvalidRequest(_))
        ));
    }
}
