#![forbid(unsafe_code)]

use std::sync::RwLock;

use cortexkit_store_types::StorageDescriptor;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use subc_client_rs::{async_trait, HandlerOutcome, ModuleHandler, RequestCtx};
use subc_protocol::{
    manifest::{
        Concurrency, ManagementOperation, ManagementOperationKind, ModuleManifest, ProviderRole,
    },
    session::{HealthReport, HealthStatus},
    ModuleHelloAckBody,
};

use crate::{
    protocol::{
        council::{EvaluateCouncilRequest, ReconcileCouncilRequest, StageCouncilRequest},
        peer::{
            AckMessageRequest, AcquireLeaseRequest, EnqueueMessageRequest, PollInboxRequest,
            ReleaseLeaseRequest, RenewLeaseRequest,
        },
        rooms::{
            BindRoomMemberRequest, CloseRoomRequest, CreatePollRequest, CreateRoomRequest,
            GetRoomRequest, GrantStageRequest, JoinRoomRequest, ObjectRoomRequest, PostRoomRequest,
            ReviseRoomRequest, VotePollRequest,
        },
    },
    service::{council::CouncilService, peer::PeerService, rooms::RoomsService, ServiceError},
    Storage, StorageError,
};

const MODULE_ID: &str = "uc-discussions";

pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder(MODULE_ID, env!("CARGO_PKG_VERSION"))
        .provides(vec![ProviderRole::ManagementSurface {
            operations: vec![
                mutate("peer.enqueue_message"),
                query("peer.poll_inbox"),
                mutate("peer.ack_message"),
                mutate("peer.acquire_lease"),
                mutate("peer.renew_lease"),
                mutate("peer.release_lease"),
                mutate("rooms.create"),
                mutate("rooms.join"),
                mutate("rooms.bind_member"),
                mutate("rooms.post"),
                mutate("rooms.object"),
                mutate("rooms.revise"),
                mutate("rooms.poll"),
                mutate("rooms.vote"),
                mutate("rooms.grant_stage"),
                mutate("rooms.close"),
                query("rooms.get"),
                mutate("council.stage"),
                mutate("council.evaluate"),
                mutate("council.reconcile"),
            ],
            config_schema: json!({
                "type": "object",
                "additionalProperties": false,
            }),
            observability: Vec::new(),
            identity_scope: Vec::new(),
            concurrency: Concurrency::ModuleManaged,
        }])
        .build()
}

fn query(name: &str) -> ManagementOperation {
    operation(name, ManagementOperationKind::Query)
}

fn mutate(name: &str) -> ManagementOperation {
    operation(name, ManagementOperationKind::Mutate)
}

fn operation(name: &str, kind: ManagementOperationKind) -> ManagementOperation {
    ManagementOperation {
        name: name.to_owned(),
        kind,
        description: None,
    }
}

struct HandlerState {
    storage: Storage,
    peer: PeerService,
    rooms: RoomsService,
    council: CouncilService,
}

impl HandlerState {
    fn new(storage: Storage) -> Self {
        Self {
            peer: PeerService::new(storage.clone()),
            rooms: RoomsService::new(storage.clone()),
            council: CouncilService::new(storage.clone()),
            storage,
        }
    }
}

pub struct DiscussionsHandler {
    state: RwLock<HandlerState>,
    storage_error: RwLock<Option<String>>,
}

impl DiscussionsHandler {
    pub fn new(storage: Storage) -> Self {
        Self {
            state: RwLock::new(HandlerState::new(storage)),
            storage_error: RwLock::new(None),
        }
    }

    pub fn try_default() -> Result<Self, StorageError> {
        Storage::open_in_memory().map(Self::new)
    }

    pub fn storage(&self) -> Result<Storage, String> {
        self.state
            .read()
            .map(|state| state.storage.clone())
            .map_err(|_| "discussions handler state lock is poisoned".to_owned())
    }

    pub fn dispatch(&self, body: &[u8]) -> HandlerOutcome {
        if let Some(message) = self.current_storage_error() {
            return handler_error("storage_initialization_failed", message);
        }

        let (operation, params) = match parse_request(body) {
            Ok(request) => request,
            Err(outcome) => return outcome,
        };
        let state = match self.state.read() {
            Ok(state) => state,
            Err(_) => {
                return handler_error(
                    "internal_error",
                    "discussions handler state lock is poisoned",
                )
            }
        };

        match operation.as_str() {
            "peer.enqueue_message" => dispatch_typed(params, |request: EnqueueMessageRequest| {
                state.peer.enqueue_message(request)
            }),
            "peer.poll_inbox" => dispatch_typed(params, |request: PollInboxRequest| {
                state.peer.poll_inbox(request)
            }),
            "peer.ack_message" => dispatch_typed(params, |request: AckMessageRequest| {
                state.peer.ack_message(request)
            }),
            "peer.acquire_lease" => dispatch_typed(params, |request: AcquireLeaseRequest| {
                state.peer.acquire_lease(request)
            }),
            "peer.renew_lease" => dispatch_typed(params, |request: RenewLeaseRequest| {
                state.peer.renew_lease(request)
            }),
            "peer.release_lease" => dispatch_typed(params, |request: ReleaseLeaseRequest| {
                state.peer.release_lease(request)
            }),
            "rooms.create" => dispatch_typed(params, |request: CreateRoomRequest| {
                state.rooms.create_room(request)
            }),
            "rooms.join" => dispatch_typed(params, |request: JoinRoomRequest| {
                state.rooms.join_room(request)
            }),
            "rooms.bind_member" => dispatch_typed(params, |request: BindRoomMemberRequest| {
                state.rooms.bind_member(request)
            }),
            "rooms.post" => {
                dispatch_typed(params, |request: PostRoomRequest| state.rooms.post(request))
            }
            "rooms.object" => dispatch_typed(params, |request: ObjectRoomRequest| {
                state.rooms.object(request)
            }),
            "rooms.revise" => dispatch_typed(params, |request: ReviseRoomRequest| {
                state.rooms.revise(request)
            }),
            "rooms.poll" => dispatch_typed(params, |request: CreatePollRequest| {
                state.rooms.create_poll(request)
            }),
            "rooms.vote" => dispatch_typed(params, |request: VotePollRequest| {
                state.rooms.vote_poll(request)
            }),
            "rooms.grant_stage" => dispatch_typed(params, |request: GrantStageRequest| {
                state.rooms.grant_stage(request)
            }),
            "rooms.close" => dispatch_typed(params, |request: CloseRoomRequest| {
                state.rooms.close_room(request)
            }),
            "rooms.get" => dispatch_typed(params, |request: GetRoomRequest| {
                state.rooms.get_room(request)
            }),
            "council.stage" => dispatch_typed(params, |request: StageCouncilRequest| {
                state.council.stage(request)
            }),
            "council.evaluate" => dispatch_typed(params, |request: EvaluateCouncilRequest| {
                state.council.evaluate(request)
            }),
            "council.reconcile" => dispatch_typed(params, |request: ReconcileCouncilRequest| {
                state.council.reconcile(request)
            }),
            _ => handler_error(
                "unknown_operation",
                format!("unknown operation '{operation}'"),
            ),
        }
    }

    fn replace_storage(&self, storage: Storage) -> Result<(), String> {
        let mut state = self
            .state
            .write()
            .map_err(|_| "discussions handler state lock is poisoned".to_owned())?;
        *state = HandlerState::new(storage);
        drop(state);

        let mut storage_error = self
            .storage_error
            .write()
            .map_err(|_| "discussions storage error lock is poisoned".to_owned())?;
        *storage_error = None;
        Ok(())
    }

    fn record_storage_error(&self, message: String) {
        tracing::error!(error = %message, "failed to initialize uc-discussions storage");
        if let Ok(mut storage_error) = self.storage_error.write() {
            *storage_error = Some(message);
        }
    }

    fn current_storage_error(&self) -> Option<String> {
        match self.storage_error.read() {
            Ok(storage_error) => storage_error.clone(),
            Err(_) => Some("discussions storage error lock is poisoned".to_owned()),
        }
    }
}

impl Default for DiscussionsHandler {
    fn default() -> Self {
        Self::try_default().expect("in-memory discussions storage should initialize")
    }
}

#[async_trait]
impl ModuleHandler for DiscussionsHandler {
    async fn handle(&self, _ctx: RequestCtx, body: Vec<u8>) -> HandlerOutcome {
        self.dispatch(&body)
    }

    async fn on_hello_ack(&self, ack: &ModuleHelloAckBody) {
        let Some(storage) = ack.storage.clone() else {
            return;
        };
        let descriptor = match serde_json::from_value::<StorageDescriptor>(storage) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.record_storage_error(format!("invalid storage descriptor: {error}"));
                return;
            }
        };
        let storage = match Storage::from_descriptor(&descriptor) {
            Ok(storage) => storage,
            Err(error) => {
                self.record_storage_error(format!("failed to open managed storage: {error}"));
                return;
            }
        };
        if let Err(error) = self.replace_storage(storage) {
            self.record_storage_error(error);
        }
    }

    async fn health(&self) -> HealthReport {
        HealthReport {
            status: HealthStatus::Ok,
            detail: Some("uc-discussions operational".into()),
            metrics: None,
        }
    }
}

fn parse_request(body: &[u8]) -> Result<(String, Value), HandlerOutcome> {
    let value = serde_json::from_slice::<Value>(body)
        .map_err(|error| handler_error("invalid_request", format!("invalid JSON: {error}")))?;
    let Value::Object(mut envelope) = value else {
        return Err(handler_error(
            "invalid_request",
            "request envelope must be a JSON object",
        ));
    };
    let operation = match envelope.remove("op").or_else(|| envelope.remove("method")) {
        Some(Value::String(operation)) if !operation.trim().is_empty() => operation,
        Some(_) => {
            return Err(handler_error(
                "invalid_request",
                "request field 'op' or 'method' must be a non-empty string",
            ))
        }
        None => {
            return Err(handler_error(
                "invalid_request",
                "request field 'op' or 'method' is required",
            ))
        }
    };
    let params = envelope
        .remove("params")
        .unwrap_or_else(|| Value::Object(envelope));
    Ok((operation, params))
}

fn dispatch_typed<Request, Response>(
    params: Value,
    call: impl FnOnce(Request) -> Result<Response, ServiceError>,
) -> HandlerOutcome
where
    Request: DeserializeOwned,
    Response: Serialize,
{
    let request = match serde_json::from_value::<Request>(params) {
        Ok(request) => request,
        Err(error) => {
            return handler_error(
                "invalid_request",
                format!("invalid operation parameters: {error}"),
            )
        }
    };
    match call(request) {
        Ok(response) => match serde_json::to_vec(&response) {
            Ok(bytes) => HandlerOutcome::Response(bytes),
            Err(error) => handler_error("encode_failed", error.to_string()),
        },
        Err(error) => service_error(error),
    }
}

fn service_error(error: ServiceError) -> HandlerOutcome {
    match error {
        ServiceError::Storage(error) => handler_error("storage_error", error.to_string()),
        ServiceError::NotFound(message) => handler_error("not_found", message),
        ServiceError::InvalidRequest(message) => handler_error("invalid_request", message),
        ServiceError::NotRoomMember { room_id, member_id } => handler_error(
            "not_room_member",
            format!("member {member_id} is not in room {room_id}"),
        ),
        ServiceError::StaleIncarnation {
            room_id,
            member_id,
            expected,
            received,
        } => handler_error(
            "stale_incarnation",
            format!(
                "member {member_id} in room {room_id} is bound to incarnation {expected}, received {received:?}"
            ),
        ),
    }
}

fn handler_error(code: &str, message: impl Into<String>) -> HandlerOutcome {
    HandlerOutcome::Error {
        code: code.to_owned(),
        message: message.into(),
    }
}
