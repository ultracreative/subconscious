use serde::{Deserialize, Serialize};

fn default_intent() -> Option<String> {
    Some("question".to_owned())
}

fn default_priority() -> Option<i32> {
    Some(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnqueueMessageRequest {
    #[serde(alias = "fromName")]
    pub from_name: String,
    #[serde(default, alias = "fromSessionID")]
    pub from_session_id: Option<String>,
    #[serde(alias = "toName")]
    pub to_name: String,
    #[serde(
        rename = "session_id",
        alias = "toSessionID",
        alias = "sessionId",
        alias = "to_session_id"
    )]
    pub target_session_id: String,
    pub body: String,
    #[serde(default = "default_intent")]
    pub intent: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: Option<i32>,
    #[serde(default)]
    pub urgency: Option<String>,
    #[serde(default, alias = "correlationId")]
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnqueueMessageResponse {
    pub ok: bool,
    pub message_id: String,
    pub state: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerMessageDto {
    pub message_id: String,
    pub thread_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub intent: String,
    pub priority: i32,
    pub state: String,
    pub delivery_receipt: Option<String>,
    pub processing_receipt: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PollInboxRequest {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    pub limit: Option<u32>,
    pub after_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PollInboxResponse {
    pub messages: Vec<PeerMessageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AckMessageRequest {
    pub message_id: String,
    pub session_id: String,
    pub receipt_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AckMessageResponse {
    pub ok: bool,
    pub message_id: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcquireLeaseRequest {
    pub resource_id: String,
    pub holder_id: String,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcquireLeaseResponse {
    pub acquired: bool,
    pub resource_id: String,
    pub holder_id: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenewLeaseRequest {
    pub resource_id: String,
    pub holder_id: String,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseLeaseRequest {
    pub resource_id: String,
    pub holder_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseLeaseResponse {
    pub released: bool,
    pub resource_id: String,
    pub holder_id: String,
}
