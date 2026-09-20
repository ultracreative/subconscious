#![forbid(unsafe_code)]

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use subc_client_rs::HandlerOutcome;
use uc_discussions::{
    protocol::rooms::{
        CloseRoomResponse, CreatePollResponse, CreateRoomResponse, GetRoomResponse,
        GrantStageResponse, JoinRoomResponse, ObjectRoomResponse, PostRoomResponse,
        ReviseRoomResponse, VotePollResponse,
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
            serde_json::from_slice(&bytes).expect("handler response should match expected type")
        }
        other => panic!("expected response outcome, got {other:?}"),
    }
}

#[test]
fn end_to_end_deliberation_and_closure_with_attributable_decisions_and_dissent() {
    let handler = DiscussionsHandler::default();

    // 1. Dispatches rooms.create (topic: "Release Strategy", goal: "Decide 0.1.0 release policy", creator: "coordinator")
    let create_resp: CreateRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.create",
            "params": {
                "topic": "Release Strategy",
                "goal": "Decide 0.1.0 release policy",
                "stage": "deliberation",
                "creator": "coordinator"
            }
        }),
    ));
    let room_id = create_resp.room_id;
    assert!(room_id.starts_with("room-"));
    assert_eq!(create_resp.status, "active");
    assert!(!create_resp.created_at.is_empty());

    // 2. Dispatches rooms.join for "agent-a", "agent-b", and "agent-c"
    let join_a: JoinRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.join",
            "params": {
                "room_id": &room_id,
                "member_id": "agent-a",
                "role": "proposer"
            }
        }),
    ));
    assert!(join_a.ok);
    assert_eq!(join_a.member.member_id, "agent-a");
    assert_eq!(join_a.member.role, "proposer");

    let join_b: JoinRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.join",
            "params": {
                "room_id": &room_id,
                "member_id": "agent-b",
                "role": "reviewer"
            }
        }),
    ));
    assert!(join_b.ok);
    assert_eq!(join_b.member.member_id, "agent-b");
    assert_eq!(join_b.member.role, "reviewer");

    let join_c: JoinRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.join",
            "params": {
                "room_id": &room_id,
                "member_id": "agent-c",
                "role": "participant"
            }
        }),
    ));
    assert!(join_c.ok);
    assert_eq!(join_c.member.member_id, "agent-c");
    assert_eq!(join_c.member.role, "participant");

    // 3. Dispatches rooms.post from "agent-a": "Ship globally immediately"
    let post_resp: PostRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.post",
            "params": {
                "room_id": &room_id,
                "author": "agent-a",
                "post_type": "proposal",
                "content": "Ship globally immediately",
                "reply_to_post_id": null
            }
        }),
    ));
    assert_eq!(post_resp.seq, 1);
    let post1_id = post_resp.post.post_id;
    assert_eq!(post_resp.post.author, "agent-a");
    assert_eq!(post_resp.post.content, "Ship globally immediately");

    // 4. Dispatches rooms.object from "agent-b" against post-1: "Rollback canary metrics are not configured"
    let obj_resp: ObjectRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.object",
            "params": {
                "room_id": &room_id,
                "post_id": &post1_id,
                "author": "agent-b",
                "reason": "Rollback canary metrics are not configured"
            }
        }),
    ));
    assert_eq!(obj_resp.objection.post_id, post1_id);
    assert_eq!(obj_resp.objection.author, "agent-b");
    assert_eq!(
        obj_resp.objection.reason,
        "Rollback canary metrics are not configured"
    );
    assert_eq!(obj_resp.objection.status, "open");

    // 5. Dispatches rooms.revise from "agent-a" for post-1: "Ship 10% canary with automated error rollback threshold"
    let rev_resp: ReviseRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.revise",
            "params": {
                "room_id": &room_id,
                "original_post_id": &post1_id,
                "author": "agent-a",
                "diff_or_content": "Ship 10% canary with automated error rollback threshold"
            }
        }),
    ));
    assert_eq!(rev_resp.revision.original_post_id, post1_id);
    assert_eq!(rev_resp.revision.author, "agent-a");
    assert_eq!(
        rev_resp.revision.diff_or_content,
        "Ship 10% canary with automated error rollback threshold"
    );

    // 6. Dispatches rooms.poll from "agent-c": "Approve revised canary plan?"
    let poll_resp: CreatePollResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.poll",
            "params": {
                "room_id": &room_id,
                "question": "Approve revised canary plan?",
                "options": ["yes", "no"],
                "created_by": "agent-c"
            }
        }),
    ));
    let poll_id = poll_resp.poll.poll_id;
    assert_eq!(poll_resp.poll.question, "Approve revised canary plan?");
    assert_eq!(poll_resp.poll.options, vec!["yes", "no"]);
    assert_eq!(poll_resp.poll.status, "active");

    // 7. Dispatches rooms.vote from "agent-a" and "agent-b"
    let vote_a: VotePollResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.vote",
            "params": {
                "poll_id": &poll_id,
                "voter": "agent-a",
                "vote": "yes"
            }
        }),
    ));
    assert!(vote_a.ok);
    assert_eq!(vote_a.voter, "agent-a");
    assert_eq!(vote_a.vote, "yes");

    let vote_b: VotePollResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.vote",
            "params": {
                "poll_id": &poll_id,
                "voter": "agent-b",
                "vote": "yes"
            }
        }),
    ));
    assert!(vote_b.ok);
    assert_eq!(vote_b.voter, "agent-b");
    assert_eq!(vote_b.vote, "yes");

    // 8. Dispatches rooms.grant_stage to "agent-a"
    let grant_resp: GrantStageResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.grant_stage",
            "params": {
                "room_id": &room_id,
                "grantee": "agent-a",
                "granted_by": "coordinator",
                "ttl_ms": 60_000
            }
        }),
    ));
    assert_eq!(grant_resp.room_id, room_id);
    assert_eq!(grant_resp.grantee, "agent-a");
    assert!(!grant_resp.expires_at.is_empty());

    // 9. Dispatches rooms.close with decisions, dissent, outstanding_actions
    let expected_decisions = vec![
        "Ship 10% canary".to_owned(),
        "Enforce error rollback threshold".to_owned(),
    ];
    let expected_dissent = vec!["Agent B requested manual canary hold".to_owned()];
    let expected_actions = vec!["Configure Prometheus canary alerting".to_owned()];

    let close_resp: CloseRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.close",
            "params": {
                "room_id": &room_id,
                "decisions": expected_decisions,
                "dissent": expected_dissent,
                "outstanding_actions": expected_actions
            }
        }),
    ));
    assert_eq!(close_resp.room_id, room_id);
    assert_eq!(close_resp.status, "closed");
    assert!(!close_resp.closed_at.is_empty());

    // 10. Dispatches rooms.get and verifies room state
    let room: GetRoomResponse = response(dispatch(
        &handler,
        json!({
            "op": "rooms.get",
            "params": {
                "room_id": &room_id
            }
        }),
    ));

    // Room status is "closed", closed_at is present
    assert_eq!(room.status, "closed");
    assert!(room.closed_at.is_some());
    assert_eq!(
        room.closed_at.as_deref(),
        Some(close_resp.closed_at.as_str())
    );

    // Exactly 3 members present with their roles
    let joined_members: Vec<_> = room
        .members
        .iter()
        .filter(|m| m.member_id != "coordinator")
        .collect();
    assert_eq!(
        joined_members.len(),
        3,
        "exactly 3 deliberation members joined"
    );

    let member_roles: HashMap<&str, &str> = room
        .members
        .iter()
        .map(|m| (m.member_id.as_str(), m.role.as_str()))
        .collect();
    assert_eq!(member_roles.get("coordinator"), Some(&"creator"));
    assert_eq!(member_roles.get("agent-a"), Some(&"proposer"));
    assert_eq!(member_roles.get("agent-b"), Some(&"reviewer"));
    assert_eq!(member_roles.get("agent-c"), Some(&"participant"));

    // All posts have strictly monotonic seq numbers (1, ...)
    assert!(!room.posts.is_empty(), "posts must not be empty");
    assert_eq!(room.posts[0].seq, 1);
    assert_eq!(room.posts[0].post_id, post1_id);
    assert_eq!(room.posts[0].author, "agent-a");
    assert_eq!(room.posts[0].content, "Ship globally immediately");
    for (idx, post) in room.posts.iter().enumerate() {
        assert_eq!(post.seq, (idx + 1) as i64);
    }
    assert!(
        room.posts.windows(2).all(|pair| pair[1].seq > pair[0].seq),
        "post sequence numbers must be strictly monotonic"
    );

    // Objections and revisions link correctly to post-1
    assert_eq!(room.objections.len(), 1);
    assert_eq!(room.objections[0].post_id, post1_id);
    assert_eq!(room.objections[0].author, "agent-b");
    assert_eq!(
        room.objections[0].reason,
        "Rollback canary metrics are not configured"
    );
    assert_eq!(room.objections[0].status, "open");

    assert_eq!(room.revisions.len(), 1);
    assert_eq!(room.revisions[0].original_post_id, post1_id);
    assert_eq!(room.revisions[0].author, "agent-a");
    assert_eq!(
        room.revisions[0].diff_or_content,
        "Ship 10% canary with automated error rollback threshold"
    );

    // Poll contains the cast votes
    assert_eq!(room.polls.len(), 1);
    let poll = &room.polls[0];
    assert_eq!(poll.poll_id, poll_id);
    assert_eq!(poll.question, "Approve revised canary plan?");
    assert_eq!(poll.options, vec!["yes", "no"]);
    assert_eq!(poll.votes.len(), 2);
    let vote_map: HashMap<&str, &str> = poll
        .votes
        .iter()
        .map(|v| (v.voter.as_str(), v.vote.as_str()))
        .collect();
    assert_eq!(vote_map.get("agent-a"), Some(&"yes"));
    assert_eq!(vote_map.get("agent-b"), Some(&"yes"));

    // Closure outcome preserves decisions, dissent, and outstanding actions identically
    assert_eq!(
        room.decisions,
        vec![
            "Ship 10% canary".to_owned(),
            "Enforce error rollback threshold".to_owned()
        ]
    );
    assert_eq!(
        room.dissent,
        vec!["Agent B requested manual canary hold".to_owned()]
    );
    assert_eq!(
        room.outstanding_actions,
        vec!["Configure Prometheus canary alerting".to_owned()]
    );

    // Active grantee verification
    assert_eq!(room.active_grantee.as_deref(), Some("agent-a"));
}
