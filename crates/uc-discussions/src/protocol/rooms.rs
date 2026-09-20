use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateRoomRequest {
    pub topic: String,
    pub goal: String,
    pub stage: String,
    pub creator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateRoomResponse {
    pub room_id: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinRoomRequest {
    pub room_id: String,
    pub member_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinRoomResponse {
    pub ok: bool,
    pub room_id: String,
    pub member: RoomMemberDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostRoomRequest {
    pub room_id: String,
    pub author: String,
    pub post_type: String,
    pub content: String,
    #[serde(default, alias = "replyToPostId")]
    pub reply_to_post_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostRoomResponse {
    pub post: RoomPostDto,
    pub seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomPostDto {
    pub seq: i64,
    pub post_id: String,
    pub author: String,
    pub post_type: String,
    pub content: String,
    pub reply_to_post_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomMemberDto {
    pub member_id: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomObjectionDto {
    pub objection_id: String,
    pub post_id: String,
    pub author: String,
    pub reason: String,
    pub status: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomRevisionDto {
    pub revision_id: String,
    pub original_post_id: String,
    pub author: String,
    pub diff_or_content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomVoteDto {
    pub voter: String,
    pub vote: String,
    pub voted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomPollDto {
    pub poll_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub votes: Vec<RoomVoteDto>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectRoomRequest {
    pub room_id: String,
    pub post_id: String,
    pub author: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectRoomResponse {
    pub objection: RoomObjectionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviseRoomRequest {
    pub room_id: String,
    pub original_post_id: String,
    pub author: String,
    pub diff_or_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviseRoomResponse {
    pub revision: RoomRevisionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreatePollRequest {
    pub room_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreatePollResponse {
    pub poll: RoomPollDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VotePollRequest {
    pub poll_id: String,
    pub voter: String,
    pub vote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VotePollResponse {
    pub ok: bool,
    pub poll_id: String,
    pub voter: String,
    pub vote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrantStageRequest {
    pub room_id: String,
    pub grantee: String,
    pub granted_by: String,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrantStageResponse {
    pub grant_id: String,
    pub room_id: String,
    pub grantee: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloseRoomRequest {
    pub room_id: String,
    pub decisions: Vec<String>,
    pub dissent: Vec<String>,
    pub outstanding_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloseRoomResponse {
    pub room_id: String,
    pub status: String,
    pub closed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetRoomRequest {
    pub room_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetRoomResponse {
    pub room_id: String,
    pub topic: String,
    pub goal: String,
    pub stage: String,
    pub active_grantee: Option<String>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub decisions: Vec<String>,
    pub dissent: Vec<String>,
    pub outstanding_actions: Vec<String>,
    pub members: Vec<RoomMemberDto>,
    pub posts: Vec<RoomPostDto>,
    pub objections: Vec<RoomObjectionDto>,
    pub revisions: Vec<RoomRevisionDto>,
    pub polls: Vec<RoomPollDto>,
}
