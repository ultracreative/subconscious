#![forbid(unsafe_code)]

use std::{path::PathBuf, thread, time::Duration};

use rusqlite::params;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use subc_client_rs::HandlerOutcome;
use uc_discussions::{
    protocol::{
        peer::{
            AckMessageResponse, AcquireLeaseResponse, EnqueueMessageResponse, PollInboxResponse,
        },
        rooms::{CreateRoomResponse, GetRoomResponse, PostRoomResponse},
    },
    DiscussionsHandler, Storage,
};

struct TempDbGuard(PathBuf);

impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

fn dispatch(handler: &DiscussionsHandler, request: Value) -> HandlerOutcome {
    handler.dispatch(
        &serde_json::to_vec(&request).expect("test request should serialize to JSON bytes"),
    )
}

fn response<T: DeserializeOwned>(outcome: HandlerOutcome) -> T {
    match outcome {
        HandlerOutcome::Response(bytes) => {
            serde_json::from_slice(&bytes).expect("handler response should match expected type")
        }
        other => panic!("expected response outcome, got {other:?}"),
    }
}

#[test]
fn chaos_and_restart_recovery_persists_state_without_duplicate_injection() {
    let db_path = std::env::temp_dir().join(format!("test_chaos_{}.db", uuid::Uuid::new_v4()));
    let _guard = TempDbGuard(db_path.clone());

    let (msg1_id, msg2_id, room_id) = {
        // =========================================================================
        // Pre-restart phase (Instance 1)
        // =========================================================================
        let storage1 = Storage::open_path(&db_path).expect("open initial storage");
        let handler1 = DiscussionsHandler::new(storage1);

        // Enqueue message 1: "msg-1" to target session "agent-x"
        let enqueue1: EnqueueMessageResponse = response(dispatch(
            &handler1,
            json!({
                "op": "peer.enqueue_message",
                "params": {
                    "from_name": "agent-y",
                    "to_name": "agent-x",
                    "session_id": "agent-x",
                    "body": "msg-1"
                }
            }),
        ));
        assert!(enqueue1.ok);
        assert_eq!(enqueue1.state, "pending");
        let msg1_id = enqueue1.message_id;

        // Enqueue message 2: "msg-2" to target session "agent-x"
        let enqueue2: EnqueueMessageResponse = response(dispatch(
            &handler1,
            json!({
                "op": "peer.enqueue_message",
                "params": {
                    "from_name": "agent-y",
                    "to_name": "agent-x",
                    "session_id": "agent-x",
                    "body": "msg-2"
                }
            }),
        ));
        assert!(enqueue2.ok);
        assert_eq!(enqueue2.state, "pending");
        let msg2_id = enqueue2.message_id;

        // Acknowledge message 1 with receipt "delivery" then "processing"
        let ack_del: AckMessageResponse = response(dispatch(
            &handler1,
            json!({
                "op": "peer.ack_message",
                "params": {
                    "message_id": &msg1_id,
                    "session_id": "agent-x",
                    "receipt_type": "delivery"
                }
            }),
        ));
        assert!(ack_del.ok);
        assert_eq!(ack_del.state, "delivered");

        let ack_proc: AckMessageResponse = response(dispatch(
            &handler1,
            json!({
                "op": "peer.ack_message",
                "params": {
                    "message_id": &msg1_id,
                    "session_id": "agent-x",
                    "receipt_type": "processing"
                }
            }),
        ));
        assert!(ack_proc.ok);
        assert_eq!(ack_proc.state, "processed");

        // Message 2 remains pending (unacknowledged)

        // Create a room and post a proposal (seq = 1)
        let create_room: CreateRoomResponse = response(dispatch(
            &handler1,
            json!({
                "op": "rooms.create",
                "params": {
                    "topic": "Chaos Recovery Strategy",
                    "goal": "Verify SQLite persistence across process restart",
                    "stage": "proposal",
                    "creator": "coordinator"
                }
            }),
        ));
        assert_eq!(create_room.status, "active");
        let room_id = create_room.room_id;

        let post: PostRoomResponse = response(dispatch(
            &handler1,
            json!({
                "op": "rooms.post",
                "params": {
                    "room_id": &room_id,
                    "author": "coordinator",
                    "post_type": "proposal",
                    "content": "Proposal for zero-loss recovery"
                }
            }),
        ));
        assert_eq!(post.seq, 1);
        assert_eq!(post.post.author, "coordinator");
        assert_eq!(post.post.content, "Proposal for zero-loss recovery");

        // Acquire short lease on resource "lock-1" with holder "worker-1" and 50ms TTL
        let lease: AcquireLeaseResponse = response(dispatch(
            &handler1,
            json!({
                "op": "peer.acquire_lease",
                "params": {
                    "resource_id": "lock-1",
                    "holder_id": "worker-1",
                    "ttl_ms": 50
                }
            }),
        ));
        assert!(lease.acquired);
        assert_eq!(lease.resource_id, "lock-1");
        assert_eq!(lease.holder_id, "worker-1");

        // Explicitly drop handler and storage to simulate sudden restart / daemon kill
        drop(handler1);

        (msg1_id, msg2_id, room_id)
    };

    // Sleep 70ms (> 50ms TTL) so the lease on "lock-1" expires before recovery
    thread::sleep(Duration::from_millis(70));

    // =========================================================================
    // Post-restart recovery phase (Instance 2)
    // =========================================================================
    let storage2 = Storage::open_path(&db_path).expect("reopen storage after restart");
    let handler2 = DiscussionsHandler::new(storage2);

    // -------------------------------------------------------------------------
    // Invariant 1: No duplicate prompt injection
    // Polling inbox for session "agent-x" with after_id = Some(msg1_id) returns
    // ONLY msg-2 (unprocessed). The already-processed msg-1 is NEVER returned.
    // -------------------------------------------------------------------------
    let poll_after_msg1: PollInboxResponse = response(dispatch(
        &handler2,
        json!({
            "op": "peer.poll_inbox",
            "params": {
                "session_id": "agent-x",
                "after_id": &msg1_id
            }
        }),
    ));
    assert_eq!(
        poll_after_msg1.messages.len(),
        1,
        "polling inbox after msg-1 must return exactly 1 message"
    );
    assert_eq!(
        poll_after_msg1.messages[0].message_id, msg2_id,
        "only msg-2 should be returned"
    );
    assert_eq!(poll_after_msg1.messages[0].body, "msg-2");
    assert_eq!(poll_after_msg1.messages[0].state, "pending");
    assert!(
        poll_after_msg1
            .messages
            .iter()
            .all(|m| m.message_id != msg1_id),
        "processed msg-1 must NEVER be returned as a duplicate"
    );

    // -------------------------------------------------------------------------
    // Invariant 2: State persistence
    // Querying msg-1 confirms its state remains 'processed' and receipts intact.
    // Querying the room confirms room, membership, and seq = 1 post are preserved.
    // -------------------------------------------------------------------------
    let raw_conn = rusqlite::Connection::open(&db_path).expect("open raw sqlite connection");
    let (persisted_state, delivery_receipt, processing_receipt): (
        String,
        Option<String>,
        Option<String>,
    ) = raw_conn
        .query_row(
            "SELECT state, delivery_receipt, processing_receipt FROM peer_messages WHERE message_id = ?1",
            params![&msg1_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("query msg-1 in sqlite");
    assert_eq!(
        persisted_state, "processed",
        "msg-1 state must remain 'processed' after restart"
    );
    assert!(
        delivery_receipt.is_some(),
        "msg-1 delivery receipt must be intact after restart"
    );
    assert!(
        processing_receipt.is_some(),
        "msg-1 processing receipt must be intact after restart"
    );

    // Also verify via full inbox poll that msg-1 is intact with processed state
    let poll_all: PollInboxResponse = response(dispatch(
        &handler2,
        json!({
            "op": "peer.poll_inbox",
            "params": {
                "session_id": "agent-x",
                "after_id": null
            }
        }),
    ));
    assert_eq!(poll_all.messages.len(), 2);
    let msg1_dto = poll_all
        .messages
        .iter()
        .find(|m| m.message_id == msg1_id)
        .expect("msg-1 present in inbox");
    assert_eq!(msg1_dto.state, "processed");
    assert!(msg1_dto.delivery_receipt.is_some());
    assert!(msg1_dto.processing_receipt.is_some());

    let msg2_dto = poll_all
        .messages
        .iter()
        .find(|m| m.message_id == msg2_id)
        .expect("msg-2 present in inbox");
    assert_eq!(msg2_dto.state, "pending");
    assert!(msg2_dto.delivery_receipt.is_none());
    assert!(msg2_dto.processing_receipt.is_none());

    // Query room to confirm room, membership, and post seq = 1 are completely preserved
    let room_resp: GetRoomResponse = response(dispatch(
        &handler2,
        json!({
            "op": "rooms.get",
            "params": {
                "room_id": &room_id
            }
        }),
    ));
    assert_eq!(room_resp.room_id, room_id);
    assert_eq!(room_resp.topic, "Chaos Recovery Strategy");
    assert_eq!(
        room_resp.goal,
        "Verify SQLite persistence across process restart"
    );
    assert_eq!(room_resp.stage, "proposal");
    assert_eq!(room_resp.status, "active");
    assert_eq!(room_resp.members.len(), 1);
    assert_eq!(room_resp.members[0].member_id, "coordinator");
    assert_eq!(room_resp.members[0].role, "creator");
    assert_eq!(room_resp.posts.len(), 1);
    assert_eq!(room_resp.posts[0].seq, 1);
    assert_eq!(room_resp.posts[0].author, "coordinator");
    assert_eq!(room_resp.posts[0].content, "Proposal for zero-loss recovery");

    // -------------------------------------------------------------------------
    // Invariant 3: Lease re-acquisition
    // After sleep > TTL, the expired lease is re-acquirable by a new holder
    // ("worker-2") without deadlock.
    // -------------------------------------------------------------------------
    let lease_reacquired: AcquireLeaseResponse = response(dispatch(
        &handler2,
        json!({
            "op": "peer.acquire_lease",
            "params": {
                "resource_id": "lock-1",
                "holder_id": "worker-2",
                "ttl_ms": 1000
            }
        }),
    ));
    assert!(
        lease_reacquired.acquired,
        "expired lease must be re-acquirable by worker-2"
    );
    assert_eq!(lease_reacquired.resource_id, "lock-1");
    assert_eq!(lease_reacquired.holder_id, "worker-2");

    drop(handler2);
}
