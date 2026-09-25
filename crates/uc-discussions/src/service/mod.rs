#![forbid(unsafe_code)]

use thiserror::Error;

use crate::StorageError;

pub mod council;
pub mod peer;
pub mod rooms;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("member {member_id} is not in room {room_id}")]
    NotRoomMember { room_id: String, member_id: String },
    #[error(
        "stale incarnation for member {member_id} in room {room_id}: expected {expected}, received {received:?}"
    )]
    StaleIncarnation {
        room_id: String,
        member_id: String,
        expected: i64,
        received: Option<i64>,
    },
}

impl From<rusqlite::Error> for ServiceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(StorageError::from(error))
    }
}
