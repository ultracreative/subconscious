use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextFileDto {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageCouncilRequest {
    pub council_id: String,
    pub name: String,
    pub question: String,
    pub intent: String,
    pub mode: String,
    pub members: Vec<String>,
    pub prompt: String,
    pub context_files: Option<Vec<ContextFileDto>>,
    pub guidance: Option<String>,
    pub deadline_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageCouncilResponse {
    pub ok: bool,
    pub council_id: String,
    pub status: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluateCouncilRequest {
    pub council_id: String,
    pub member_name: String,
    pub status: String,
    pub response_block: Option<String>,
    pub error: Option<String>,
    pub token_cost_nanodollars: Option<i128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluateCouncilResponse {
    pub ok: bool,
    pub council_id: String,
    pub member_name: String,
    pub status: String,
    pub all_members_terminal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconcileCouncilRequest {
    pub council_id: String,
    pub declared_members: Vec<String>,
    pub synthesis: String,
    pub agreement_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconcileCouncilResponse {
    pub ok: bool,
    pub council_id: String,
    pub status: String,
    pub completed_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetCouncilRequest {
    pub council_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouncilMemberStateDto {
    pub member_name: String,
    pub status: String,
    pub updated_at: String,
    pub response_block: Option<String>,
    pub error: Option<String>,
    pub token_cost_nanodollars: Option<i128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouncilRunDto {
    pub council_id: String,
    pub name: String,
    pub question: String,
    pub intent: Option<String>,
    pub mode: Option<String>,
    pub members: Vec<String>,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub synthesis: Option<String>,
    pub agreement_level: Option<String>,
    pub member_states: Vec<CouncilMemberStateDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetCouncilResponse {
    pub ok: bool,
    pub council: CouncilRunDto,
}
