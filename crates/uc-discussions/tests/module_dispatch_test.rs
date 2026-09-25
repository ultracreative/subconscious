#![forbid(unsafe_code)]

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use subc_client_rs::HandlerOutcome;
use subc_protocol::manifest::{Concurrency, ProviderRole};
use uc_discussions::{
    manifest,
    protocol::{
        council::{EvaluateCouncilResponse, ReconcileCouncilResponse, StageCouncilResponse},
        peer::{EnqueueMessageResponse, PollInboxResponse},
        rooms::{CreateRoomResponse, PostRoomResponse},
    },
    DiscussionsHandler,
};

fn dispatch(handler: &DiscussionsHandler, request: Value) -> HandlerOutcome {
    handler.dispatch(
        &serde_json::to_vec(&request).expect("test request should serialize to JSON bytes"),
    )
}

fn response<T: DeserializeOwned>(outcome: HandlerOutcome) -> T {
    match outcome {
        HandlerOutcome::Response(bytes) => {
            serde_json::from_slice(&bytes).expect("handler response should match the expected type")
        }
        other => panic!("expected response outcome, got {other:?}"),
    }
}

#[test]
fn manifest_declares_all_management_surface_operations() {
    let manifest = manifest();
    assert_eq!(manifest.module_id, "uc-discussions");
    let [ProviderRole::ManagementSurface {
        operations,
        concurrency,
        ..
    }] = manifest.provides.as_slice()
    else {
        panic!("manifest should declare exactly one management surface");
    };

    assert_eq!(*concurrency, Concurrency::ModuleManaged);
    assert_eq!(
        operations
            .iter()
            .map(|operation| operation.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "peer.enqueue_message",
            "peer.poll_inbox",
            "peer.ack_message",
            "peer.acquire_lease",
            "peer.renew_lease",
            "peer.release_lease",
            "rooms.create",
            "rooms.join",
            "rooms.bind_member",
            "rooms.post",
            "rooms.object",
            "rooms.revise",
            "rooms.poll",
            "rooms.vote",
            "rooms.grant_stage",
            "rooms.close",
            "rooms.get",
            "council.stage",
            "council.evaluate",
            "council.reconcile",
        ]
    );
}

#[test]
fn enqueue_message_dispatch_persists_message() {
    let handler = DiscussionsHandler::default();
    let enqueued: EnqueueMessageResponse = response(dispatch(
        &handler,
        json!({
            "op": "peer.enqueue_message",
            "params": {
                "from_name": "sender",
                "to_name": "recipient",
                "session_id": "recipient-session",
                "body": "hello from dispatch"
            }
        }),
    ));
    assert!(enqueued.ok);
    assert_eq!(enqueued.state, "pending");

    let inbox: PollInboxResponse = response(dispatch(
        &handler,
        json!({
            "op": "peer.poll_inbox",
            "session_id": "recipient-session",
            "limit": 10,
            "after_id": null
        }),
    ));
    assert_eq!(inbox.messages.len(), 1);
    assert_eq!(inbox.messages[0].message_id, enqueued.message_id);
    assert_eq!(inbox.messages[0].body, "hello from dispatch");
}

#[test]
fn room_dispatch_allocates_monotonic_post_sequences() {
    let handler = DiscussionsHandler::default();
    let room: CreateRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.create",
            "params": {
                "topic": "Release",
                "goal": "Decide whether to ship",
                "stage": "discussion",
                "creator": "alice"
            }
        }),
    ));

    let first: PostRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.post",
            "room_id": room.room_id,
            "author": "alice",
            "post_type": "proposal",
            "content": "Ship after verification",
            "reply_to_post_id": null
        }),
    ));
    let second: PostRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.post",
            "params": {
                "room_id": room.room_id,
                "author": "bob",
                "post_type": "evidence",
                "content": "All checks passed",
                "reply_to_post_id": first.post.post_id
            }
        }),
    ));

    assert_eq!(first.seq, 1);
    assert_eq!(second.seq, 2);
    assert!(second.seq > first.seq);
}

#[test]
fn council_dispatch_requires_all_terminal_members_before_reconcile() {
    let handler = DiscussionsHandler::default();
    let staged: StageCouncilResponse = response(dispatch(
        &handler,
        json!({
            "op": "council.stage",
            "params": {
                "council_id": "release-council",
                "name": "Release council",
                "question": "Should we ship?",
                "intent": "decision",
                "mode": "plan",
                "members": ["alpha", "beta"],
                "prompt": "Evaluate release readiness",
                "context_files": null,
                "guidance": null,
                "deadline_ms": null
            }
        }),
    ));
    assert_eq!(staged.status, "staged");

    let alpha: EvaluateCouncilResponse = response(dispatch(
        &handler,
        json!({
            "op": "council.evaluate",
            "council_id": "release-council",
            "member_name": "alpha",
            "status": "completed",
            "response_block": "approve",
            "error": null,
            "token_cost_nanodollars": 10
        }),
    ));
    assert!(!alpha.all_members_terminal);

    let early_reconcile = dispatch(
        &handler,
        json!({
            "op": "council.reconcile",
            "params": {
                "council_id": "release-council",
                "declared_members": ["alpha", "beta"],
                "synthesis": "ship",
                "agreement_level": "strong"
            }
        }),
    );
    match early_reconcile {
        HandlerOutcome::Error { code, message } => {
            assert_eq!(code, "invalid_request");
            assert!(message.contains("beta"));
            assert!(message.contains("non-terminal"));
        }
        other => panic!("expected non-terminal reconciliation error, got {other:?}"),
    }

    let beta: EvaluateCouncilResponse = response(dispatch(
        &handler,
        json!({
            "op": "council.evaluate",
            "params": {
                "council_id": "release-council",
                "member_name": "beta",
                "status": "failed",
                "response_block": null,
                "error": "model unavailable",
                "token_cost_nanodollars": null
            }
        }),
    ));
    assert!(beta.all_members_terminal);

    let reconciled: ReconcileCouncilResponse = response(dispatch(
        &handler,
        json!({
            "op": "council.reconcile",
            "council_id": "release-council",
            "declared_members": ["alpha", "beta"],
            "synthesis": "ship with alpha's approval and beta's failure recorded",
            "agreement_level": "partial"
        }),
    ));
    assert!(reconciled.ok);
    assert_eq!(reconciled.status, "completed");
}

#[test]
fn unknown_operation_returns_clear_error() {
    let handler = DiscussionsHandler::default();
    match dispatch(&handler, json!({ "op": "rooms.no_such_operation" })) {
        HandlerOutcome::Error { code, message } => {
            assert_eq!(code, "unknown_operation");
            assert_eq!(message, "unknown operation 'rooms.no_such_operation'");
        }
        other => panic!("expected unknown-operation error, got {other:?}"),
    }
}
