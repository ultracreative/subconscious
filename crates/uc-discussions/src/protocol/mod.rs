#![forbid(unsafe_code)]

pub mod council;
pub mod peer;
pub mod rooms;

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::json;

    use super::{council::*, peer::*, rooms::*};

    fn assert_round_trip<T>(value: T)
    where
        T: Debug + PartialEq + Serialize + DeserializeOwned,
    {
        let encoded = serde_json::to_value(&value).expect("protocol value should serialize");
        let decoded: T =
            serde_json::from_value(encoded).expect("serialized protocol value should deserialize");
        assert_eq!(decoded, value);
    }

    #[test]
    fn enqueue_target_session_accepts_all_supported_parameter_names() {
        for (key, expected) in [
            ("session_id", "abc"),
            ("toSessionID", "xyz"),
            ("sessionId", "def"),
            ("to_session_id", "ghi"),
        ] {
            let mut value = json!({
                "from_name": "sender",
                "to_name": "recipient",
                "body": "hello"
            });
            value
                .as_object_mut()
                .expect("test fixture should be an object")
                .insert(key.to_owned(), json!(expected));

            let request: EnqueueMessageRequest =
                serde_json::from_value(value).expect("target session alias should deserialize");
            assert_eq!(request.target_session_id, expected);
            assert_eq!(request.intent.as_deref(), Some("question"));
            assert_eq!(request.priority, Some(0));
        }
    }

    #[test]
    fn peer_contracts_round_trip() {
        let message = PeerMessageDto {
            message_id: "message-1".into(),
            thread_id: "thread-1".into(),
            from_agent: "sender".into(),
            to_agent: "recipient".into(),
            body: "hello".into(),
            intent: "question".into(),
            priority: 3,
            state: "pending".into(),
            delivery_receipt: Some("delivered".into()),
            processing_receipt: Some("processed".into()),
            created_at: "2026-09-19T00:00:00Z".into(),
        };

        assert_round_trip(EnqueueMessageRequest {
            from_name: "sender".into(),
            from_session_id: Some("source-session".into()),
            to_name: "recipient".into(),
            target_session_id: "target-session".into(),
            body: "hello".into(),
            intent: Some("question".into()),
            priority: Some(3),
            urgency: Some("normal".into()),
            correlation_id: Some("correlation-1".into()),
        });
        assert_round_trip(EnqueueMessageResponse {
            ok: true,
            message_id: "message-1".into(),
            state: "pending".into(),
            timestamp: "2026-09-19T00:00:00Z".into(),
        });
        assert_round_trip(message.clone());
        assert_round_trip(PollInboxRequest {
            session_id: "target-session".into(),
            limit: Some(25),
            after_id: Some("message-0".into()),
        });
        assert_round_trip(PollInboxResponse {
            messages: vec![message],
        });
        assert_round_trip(AckMessageRequest {
            message_id: "message-1".into(),
            session_id: "target-session".into(),
            receipt_type: "delivery".into(),
        });
        assert_round_trip(AckMessageResponse {
            ok: true,
            message_id: "message-1".into(),
            state: "delivered".into(),
        });
        assert_round_trip(AcquireLeaseRequest {
            resource_id: "inbox:target-session".into(),
            holder_id: "worker-1".into(),
            ttl_ms: 30_000,
        });
        assert_round_trip(AcquireLeaseResponse {
            acquired: true,
            resource_id: "inbox:target-session".into(),
            holder_id: "worker-1".into(),
            expires_at: Some("2026-09-19T00:00:30Z".into()),
        });
        assert_round_trip(RenewLeaseRequest {
            resource_id: "inbox:target-session".into(),
            holder_id: "worker-1".into(),
            ttl_ms: 30_000,
        });
        assert_round_trip(ReleaseLeaseRequest {
            resource_id: "inbox:target-session".into(),
            holder_id: "worker-1".into(),
        });
        assert_round_trip(ReleaseLeaseResponse {
            released: true,
            resource_id: "inbox:target-session".into(),
            holder_id: "worker-1".into(),
        });
    }

    #[test]
    fn room_contracts_round_trip() {
        let member = RoomMemberDto {
            member_id: "member-1".into(),
            role: "participant".into(),
            joined_at: "2026-09-19T00:00:00Z".into(),
            project_id: None,
            session_id: None,
            agent: None,
            model: None,
            delivery_mode: None,
            incarnation: None,
        };
        let post = RoomPostDto {
            seq: 1,
            post_id: "post-1".into(),
            author: "member-1".into(),
            post_type: "proposal".into(),
            content: "Ship it".into(),
            reply_to_post_id: None,
            created_at: "2026-09-19T00:01:00Z".into(),
        };
        let objection = RoomObjectionDto {
            objection_id: "objection-1".into(),
            post_id: "post-1".into(),
            author: "member-2".into(),
            reason: "Needs evidence".into(),
            status: "open".into(),
            resolved_at: None,
        };
        let revision = RoomRevisionDto {
            revision_id: "revision-1".into(),
            original_post_id: "post-1".into(),
            author: "member-1".into(),
            diff_or_content: "Ship after verification".into(),
            created_at: "2026-09-19T00:02:00Z".into(),
        };
        let poll = RoomPollDto {
            poll_id: "poll-1".into(),
            question: "Approve?".into(),
            options: vec!["yes".into(), "no".into()],
            votes: vec![RoomVoteDto {
                voter: "member-1".into(),
                vote: "yes".into(),
                voted_at: "2026-09-19T00:03:00Z".into(),
            }],
            status: "active".into(),
            created_at: "2026-09-19T00:02:30Z".into(),
            closed_at: None,
        };

        assert_round_trip(CreateRoomRequest {
            topic: "Release".into(),
            goal: "Reach a decision".into(),
            stage: "discussion".into(),
            creator: "member-1".into(),
        });
        assert_round_trip(CreateRoomResponse {
            room_id: "room-1".into(),
            status: "active".into(),
            created_at: "2026-09-19T00:00:00Z".into(),
        });
        assert_round_trip(JoinRoomRequest {
            room_id: "room-1".into(),
            member_id: "member-1".into(),
            role: "participant".into(),
        });
        assert_round_trip(JoinRoomResponse {
            ok: true,
            room_id: "room-1".into(),
            member: member.clone(),
        });
        assert_round_trip(PostRoomRequest {
            room_id: "room-1".into(),
            author: "member-1".into(),
            incarnation: None,
            post_type: "proposal".into(),
            content: "Ship it".into(),
            reply_to_post_id: None,
        });
        assert_round_trip(PostRoomResponse {
            post: post.clone(),
            seq: 1,
        });
        assert_round_trip(post.clone());
        assert_round_trip(member.clone());
        assert_round_trip(objection.clone());
        assert_round_trip(revision.clone());
        assert_round_trip(poll.clone());
        assert_round_trip(ObjectRoomRequest {
            room_id: "room-1".into(),
            post_id: "post-1".into(),
            author: "member-2".into(),
            reason: "Needs evidence".into(),
        });
        assert_round_trip(ObjectRoomResponse {
            objection: objection.clone(),
        });
        assert_round_trip(ReviseRoomRequest {
            room_id: "room-1".into(),
            original_post_id: "post-1".into(),
            author: "member-1".into(),
            diff_or_content: "Ship after verification".into(),
        });
        assert_round_trip(ReviseRoomResponse {
            revision: revision.clone(),
        });
        assert_round_trip(CreatePollRequest {
            room_id: "room-1".into(),
            question: "Approve?".into(),
            options: vec!["yes".into(), "no".into()],
            created_by: "member-1".into(),
        });
        assert_round_trip(CreatePollResponse { poll: poll.clone() });
        assert_round_trip(VotePollRequest {
            poll_id: "poll-1".into(),
            voter: "member-1".into(),
            vote: "yes".into(),
        });
        assert_round_trip(VotePollResponse {
            ok: true,
            poll_id: "poll-1".into(),
            voter: "member-1".into(),
            vote: "yes".into(),
        });
        assert_round_trip(GrantStageRequest {
            room_id: "room-1".into(),
            grantee: "member-1".into(),
            granted_by: "moderator".into(),
            ttl_ms: 30_000,
        });
        assert_round_trip(GrantStageResponse {
            grant_id: "grant-1".into(),
            room_id: "room-1".into(),
            grantee: "member-1".into(),
            expires_at: "2026-09-19T00:04:00Z".into(),
        });
        assert_round_trip(CloseRoomRequest {
            room_id: "room-1".into(),
            decisions: vec!["Ship after verification".into()],
            dissent: vec!["Prefer another review".into()],
            outstanding_actions: vec!["Run release checks".into()],
        });
        assert_round_trip(CloseRoomResponse {
            room_id: "room-1".into(),
            status: "closed".into(),
            closed_at: "2026-09-19T00:05:00Z".into(),
        });
        assert_round_trip(GetRoomRequest {
            room_id: "room-1".into(),
        });
        assert_round_trip(GetRoomResponse {
            room_id: "room-1".into(),
            topic: "Release".into(),
            goal: "Reach a decision".into(),
            stage: "discussion".into(),
            active_grantee: Some("member-1".into()),
            status: "closed".into(),
            created_at: "2026-09-19T00:00:00Z".into(),
            closed_at: Some("2026-09-19T00:05:00Z".into()),
            decisions: vec!["Ship after verification".into()],
            dissent: vec!["Prefer another review".into()],
            outstanding_actions: vec!["Run release checks".into()],
            members: vec![member],
            posts: vec![post],
            objections: vec![objection],
            revisions: vec![revision],
            polls: vec![poll],
        });
    }

    #[test]
    fn council_contracts_round_trip() {
        let context_file = ContextFileDto {
            path: "README.md".into(),
            content: "Context".into(),
        };

        assert_round_trip(context_file.clone());
        assert_round_trip(StageCouncilRequest {
            council_id: "council-1".into(),
            name: "Release council".into(),
            question: "Should we ship?".into(),
            intent: "decision".into(),
            mode: "plan".into(),
            members: vec!["member-1".into(), "member-2".into()],
            prompt: "Review the release".into(),
            context_files: Some(vec![context_file]),
            guidance: Some("Be concise".into()),
            deadline_ms: Some(60_000),
        });
        assert_round_trip(StageCouncilResponse {
            ok: true,
            council_id: "council-1".into(),
            status: "staged".into(),
            started_at: "2026-09-19T00:00:00Z".into(),
        });
        assert_round_trip(EvaluateCouncilRequest {
            council_id: "council-1".into(),
            member_name: "member-1".into(),
            status: "completed".into(),
            response_block: Some("Approve".into()),
            error: None,
            token_cost_nanodollars: Some(42),
        });
        assert_round_trip(EvaluateCouncilResponse {
            ok: true,
            council_id: "council-1".into(),
            member_name: "member-1".into(),
            status: "completed".into(),
            all_members_terminal: false,
        });
        assert_round_trip(ReconcileCouncilRequest {
            council_id: "council-1".into(),
            declared_members: vec!["member-1".into(), "member-2".into()],
            synthesis: "Ship after checks".into(),
            agreement_level: Some("strong".into()),
        });
        assert_round_trip(ReconcileCouncilResponse {
            ok: true,
            council_id: "council-1".into(),
            status: "completed".into(),
            completed_at: "2026-09-19T00:10:00Z".into(),
        });
    }
}
