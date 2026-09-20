#![forbid(unsafe_code)]

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use subc_client_rs::HandlerOutcome;
use uc_discussions::{
    protocol::{
        council::{EvaluateCouncilResponse, ReconcileCouncilResponse, StageCouncilResponse},
        peer::{AckMessageResponse, EnqueueMessageResponse, PollInboxResponse},
        rooms::{
            CreatePollResponse, CreateRoomResponse, GetRoomResponse, GrantStageResponse,
            JoinRoomResponse, ObjectRoomResponse, PostRoomResponse, ReviseRoomResponse,
            VotePollResponse,
        },
    },
    storage::snapshot::DiscussionsSnapshot,
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
fn zero_loss_export_import_migration_snapshot_proof() {
    let db1_path = std::env::temp_dir().join(format!("test_snapshot_db1_{}.db", uuid::Uuid::new_v4()));
    let _guard1 = TempDbGuard(db1_path.clone());

    let storage1 = Storage::open_path(&db1_path).expect("open db1 storage");
    let handler1 = DiscussionsHandler::new(storage1.clone());

    // =========================================================================
    // 1. Seed Database 1 with rich discussion state
    // =========================================================================

    // A. Peer messages: pending, delivered, processed with receipts
    let enq1: EnqueueMessageResponse = response(dispatch(
        &handler1,
        json!({
            "op": "peer.enqueue_message",
            "params": {
                "from_name": "agent-alpha",
                "to_name": "agent-beta",
                "session_id": "agent-beta",
                "body": "First message - pending state",
                "intent": "question",
                "priority": 10
            }
        }),
    ));
    assert!(enq1.ok);
    assert_eq!(enq1.state, "pending");
    let msg1_id = enq1.message_id;

    let enq2: EnqueueMessageResponse = response(dispatch(
        &handler1,
        json!({
            "op": "peer.enqueue_message",
            "params": {
                "from_name": "agent-alpha",
                "to_name": "agent-beta",
                "session_id": "agent-beta",
                "body": "Second message - delivered state",
                "intent": "inform",
                "priority": 5
            }
        }),
    ));
    assert!(enq2.ok);
    let msg2_id = enq2.message_id;

    let ack_del2: AckMessageResponse = response(dispatch(
        &handler1,
        json!({
            "op": "peer.ack_message",
            "params": {
                "message_id": &msg2_id,
                "session_id": "agent-beta",
                "receipt_type": "delivery"
            }
        }),
    ));
    assert!(ack_del2.ok);
    assert_eq!(ack_del2.state, "delivered");

    let enq3: EnqueueMessageResponse = response(dispatch(
        &handler1,
        json!({
            "op": "peer.enqueue_message",
            "params": {
                "from_name": "agent-gamma",
                "to_name": "agent-beta",
                "session_id": "agent-beta",
                "body": "Third message - processed state",
                "intent": "command",
                "priority": 1
            }
        }),
    ));
    assert!(enq3.ok);
    let msg3_id = enq3.message_id;

    let ack_del3: AckMessageResponse = response(dispatch(
        &handler1,
        json!({
            "op": "peer.ack_message",
            "params": {
                "message_id": &msg3_id,
                "session_id": "agent-beta",
                "receipt_type": "delivery"
            }
        }),
    ));
    assert!(ack_del3.ok);

    let ack_proc3: AckMessageResponse = response(dispatch(
        &handler1,
        json!({
            "op": "peer.ack_message",
            "params": {
                "message_id": &msg3_id,
                "session_id": "agent-beta",
                "receipt_type": "processing"
            }
        }),
    ));
    assert!(ack_proc3.ok);
    assert_eq!(ack_proc3.state, "processed");

    // B. Multi-member room with sequenced posts (1, 2, 3), objections, revisions, polls, votes, stage grants
    let create_room_resp: CreateRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.create",
            "params": {
                "topic": "Zero-Loss Migration Snapshot Design",
                "goal": "Ensure lossless schema export/import between SQLite stores",
                "stage": "deliberation",
                "creator": "alice"
            }
        }),
    ));
    let room_id = create_room_resp.room_id;

    let join_bob: JoinRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.join",
            "params": {
                "room_id": &room_id,
                "member_id": "bob",
                "role": "reviewer"
            }
        }),
    ));
    assert!(join_bob.ok);

    let join_carol: JoinRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.join",
            "params": {
                "room_id": &room_id,
                "member_id": "carol",
                "role": "participant"
            }
        }),
    ));
    assert!(join_carol.ok);

    let post1: PostRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.post",
            "params": {
                "room_id": &room_id,
                "author": "alice",
                "post_type": "proposal",
                "content": "Proposal: Structured snapshot covering all 12 durable tables."
            }
        }),
    ));
    assert_eq!(post1.seq, 1);
    let post1_id = post1.post.post_id;

    let post2: PostRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.post",
            "params": {
                "room_id": &room_id,
                "author": "bob",
                "post_type": "critique",
                "content": "Critique: Must run inside an immediate transaction for atomicity.",
                "reply_to_post_id": &post1_id
            }
        }),
    ));
    assert_eq!(post2.seq, 2);
    let post2_id = post2.post.post_id;

    let post3: PostRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.post",
            "params": {
                "room_id": &room_id,
                "author": "carol",
                "post_type": "endorsement",
                "content": "Endorsement: Immediate transactions prevent WAL serialization conflicts.",
                "reply_to_post_id": &post2_id
            }
        }),
    ));
    assert_eq!(post3.seq, 3);

    let objection: ObjectRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.object",
            "params": {
                "room_id": &room_id,
                "post_id": &post1_id,
                "author": "bob",
                "reason": "Missing proof of monotonic sequence continuation after restore"
            }
        }),
    ));
    assert_eq!(objection.objection.status, "open");

    let revision: ReviseRoomResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.revise",
            "params": {
                "room_id": &room_id,
                "original_post_id": &post1_id,
                "author": "alice",
                "diff_or_content": "Added specification for post sequence monotonicity check post-import."
            }
        }),
    ));
    assert_eq!(revision.revision.author, "alice");

    let poll: CreatePollResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.poll",
            "params": {
                "room_id": &room_id,
                "question": "Approve snapshot export/import RFC?",
                "options": ["Approve", "Request Changes"],
                "created_by": "alice"
            }
        }),
    ));
    let poll_id = poll.poll.poll_id;

    let vote1: VotePollResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.vote",
            "params": {
                "poll_id": &poll_id,
                "voter": "alice",
                "vote": "Approve"
            }
        }),
    ));
    assert!(vote1.ok);

    let vote2: VotePollResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.vote",
            "params": {
                "poll_id": &poll_id,
                "voter": "bob",
                "vote": "Approve"
            }
        }),
    ));
    assert!(vote2.ok);

    let grant: GrantStageResponse = response(dispatch(
        &handler1,
        json!({
            "op": "rooms.grant_stage",
            "params": {
                "room_id": &room_id,
                "grantee": "bob",
                "granted_by": "alice",
                "ttl_ms": 120_000
            }
        }),
    ));
    assert_eq!(grant.grantee, "bob");

    // C. Council run with member states and completed synthesis
    let council_id = "council-snap-001";
    let stage_resp: StageCouncilResponse = response(dispatch(
        &handler1,
        json!({
            "op": "council.stage",
            "params": {
                "council_id": council_id,
                "name": "Snapshot Verification Council",
                "question": "Does DiscussionsSnapshot guarantee zero-loss across 12 tables?",
                "intent": "verification",
                "mode": "unanimous",
                "members": ["alice", "bob"],
                "prompt": "Review and verify the snapshot implementation for zero-loss properties."
            }
        }),
    ));
    assert!(stage_resp.ok);
    assert_eq!(stage_resp.status, "staged");

    let eval_alice: EvaluateCouncilResponse = response(dispatch(
        &handler1,
        json!({
            "op": "council.evaluate",
            "params": {
                "council_id": council_id,
                "member_name": "alice",
                "status": "completed",
                "response_block": "All table schemas verified. Deterministic ordering guaranteed."
            }
        }),
    ));
    assert!(eval_alice.ok);
    assert!(!eval_alice.all_members_terminal);

    let eval_bob: EvaluateCouncilResponse = response(dispatch(
        &handler1,
        json!({
            "op": "council.evaluate",
            "params": {
                "council_id": council_id,
                "member_name": "bob",
                "status": "completed",
                "response_block": "Import transaction atomicity confirmed. Monotonic post sequences preserved."
            }
        }),
    ));
    assert!(eval_bob.ok);
    assert!(eval_bob.all_members_terminal);

    let reconcile: ReconcileCouncilResponse = response(dispatch(
        &handler1,
        json!({
            "op": "council.reconcile",
            "params": {
                "council_id": council_id,
                "declared_members": ["alice", "bob"],
                "synthesis": "Council fully approves zero-loss snapshot export and import implementation.",
                "agreement_level": "unanimous"
            }
        }),
    ));
    assert!(reconcile.ok);
    assert_eq!(reconcile.status, "completed");

    // =========================================================================
    // 2. Export snapshot on Database 1 and serialize to JSON
    // =========================================================================
    let snapshot1 = storage1.export_snapshot().expect("export snapshot from db1");

    // Verify all 12 tables have records in snapshot1
    assert!(!snapshot1.peer_threads.is_empty(), "peer_threads must not be empty");
    assert_eq!(snapshot1.peer_messages.len(), 3, "peer_messages must contain 3 messages");
    assert_eq!(snapshot1.rooms.len(), 1, "rooms must contain 1 room");
    assert_eq!(snapshot1.room_members.len(), 3, "room_members must contain 3 members");
    assert_eq!(snapshot1.room_posts.len(), 3, "room_posts must contain 3 posts");
    assert_eq!(snapshot1.room_objections.len(), 1, "room_objections must contain 1 objection");
    assert_eq!(snapshot1.room_revisions.len(), 1, "room_revisions must contain 1 revision");
    assert_eq!(snapshot1.room_polls.len(), 1, "room_polls must contain 1 poll");
    assert_eq!(snapshot1.room_votes.len(), 2, "room_votes must contain 2 votes");
    assert_eq!(snapshot1.room_stage_grants.len(), 1, "room_stage_grants must contain 1 grant");
    assert_eq!(snapshot1.council_runs.len(), 1, "council_runs must contain 1 run");
    assert_eq!(snapshot1.council_member_states.len(), 2, "council_member_states must contain 2 member states");

    // Serialize to JSON and deserialize back
    let json_bytes = serde_json::to_vec_pretty(&snapshot1).expect("serialize snapshot to JSON");
    let deserialized_snapshot: DiscussionsSnapshot =
        serde_json::from_slice(&json_bytes).expect("deserialize snapshot from JSON");
    assert_eq!(
        snapshot1, deserialized_snapshot,
        "JSON serialization round-trip must be lossless"
    );

    // =========================================================================
    // 3. Initialize fresh Database 2 and call import_snapshot
    // =========================================================================
    let db2_path = std::env::temp_dir().join(format!("test_snapshot_db2_{}.db", uuid::Uuid::new_v4()));
    let _guard2 = TempDbGuard(db2_path.clone());

    let storage2 = Storage::open_path(&db2_path).expect("open db2 storage");
    storage2
        .import_snapshot(&deserialized_snapshot)
        .expect("import snapshot into fresh db2");

    // =========================================================================
    // 4. Prove ZERO LOSS
    // =========================================================================

    // A. Export from Database 2 and assert exact snapshot equality
    let snapshot2 = storage2.export_snapshot().expect("export snapshot from db2");
    assert_eq!(
        snapshot1, snapshot2,
        "ZERO LOSS: snapshot exported from db2 must match snapshot exported from db1 exactly"
    );

    // B. Insert new post in Database 2 and verify sequence continues from seq = 4
    let post4_id = format!("post-{}", uuid::Uuid::new_v4());
    let seq4 = storage2
        .insert_room_post(
            &room_id,
            &post4_id,
            "alice",
            "update",
            "Fourth post proving sequence continuation post-migration",
            Some(&post1_id),
        )
        .expect("insert new post on db2");
    assert_eq!(
        seq4, 4,
        "Room post sequence on db2 must continue monotonically from seq = 4"
    );

    // Verify seq 4 is retrievable through handler on db2
    let handler2 = DiscussionsHandler::new(storage2.clone());
    let room_get: GetRoomResponse = response(dispatch(
        &handler2,
        json!({
            "op": "rooms.get",
            "params": {
                "room_id": &room_id
            }
        }),
    ));
    assert_eq!(room_get.posts.len(), 4);
    assert_eq!(room_get.posts[3].seq, 4);
    assert_eq!(room_get.posts[3].content, "Fourth post proving sequence continuation post-migration");

    // C. Verify council member states and all_members_terminal function identically on Database 2
    let declared = ["alice", "bob"];
    let is_terminal = storage2
        .all_members_terminal(council_id, &declared)
        .expect("check all members terminal on db2");
    assert!(
        is_terminal,
        "Council all_members_terminal on db2 must return true for completed members"
    );

    // Adding an unfinished member makes all_members_terminal false
    storage2
        .record_member_state(council_id, "carol", "running", None, None)
        .expect("record running member on db2");
    let is_terminal_with_running = storage2
        .all_members_terminal(council_id, &["alice", "bob", "carol"])
        .expect("check terminal with running member on db2");
    assert!(
        !is_terminal_with_running,
        "Council all_members_terminal on db2 must return false when any member is running"
    );

    // Marking carol completed restores terminal state
    storage2
        .record_member_state(
            council_id,
            "carol",
            "completed",
            Some("Verified on secondary node"),
            None,
        )
        .expect("record completed member on db2");
    let is_terminal_after_completion = storage2
        .all_members_terminal(council_id, &["alice", "bob", "carol"])
        .expect("check terminal after all completed on db2");
    assert!(
        is_terminal_after_completion,
        "Council all_members_terminal on db2 must return true once all members are terminal"
    );

    // D. Verify peer messages on Database 2 via poll_inbox
    let inbox: PollInboxResponse = response(dispatch(
        &handler2,
        json!({
            "op": "peer.poll_inbox",
            "params": {
                "session_id": "agent-beta"
            }
        }),
    ));
    assert_eq!(inbox.messages.len(), 3);
    assert_eq!(inbox.messages[0].message_id, msg1_id);
    assert_eq!(inbox.messages[0].state, "pending");
    assert_eq!(inbox.messages[1].message_id, msg2_id);
    assert_eq!(inbox.messages[1].state, "delivered");
    assert!(inbox.messages[1].delivery_receipt.is_some());
    assert_eq!(inbox.messages[2].message_id, msg3_id);
    assert_eq!(inbox.messages[2].state, "processed");
    assert!(inbox.messages[2].delivery_receipt.is_some());
    assert!(inbox.messages[2].processing_receipt.is_some());

    // E. Verify in-memory database import also preserves zero-loss
    let storage_mem = Storage::open_in_memory().expect("open in-memory storage");
    storage_mem
        .import_snapshot(&snapshot1)
        .expect("import snapshot into in-memory storage");
    let snapshot_mem = storage_mem.export_snapshot().expect("export from in-memory");
    assert_eq!(
        snapshot1, snapshot_mem,
        "In-memory storage import/export must also be zero-loss"
    );
}
