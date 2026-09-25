#![forbid(unsafe_code)]

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use subc_client_rs::HandlerOutcome;
use uc_discussions::DiscussionsHandler;

fn dispatch(handler: &DiscussionsHandler, request: Value) -> HandlerOutcome {
    handler.dispatch(&serde_json::to_vec(&request).expect("serialize request"))
}

fn response<T: DeserializeOwned>(outcome: HandlerOutcome) -> Result<T, String> {
    match outcome {
        HandlerOutcome::Response(bytes) => {
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())
        }
        HandlerOutcome::Error { code, message } => Err(format!("{code}: {message}")),
        HandlerOutcome::ErrorWithDetail { code, message, .. } => Err(format!("{code}: {message}")),
        HandlerOutcome::Streamed => Err("unexpected streamed response".to_owned()),
    }
}

#[test]
fn woken_colleague_binding_rejects_stale_incarnation() {
    let handler = DiscussionsHandler::default();
    let created: Value = response(dispatch(
        &handler,
        json!({
            "op": "rooms.create",
            "params": {
                "topic": "Cross-project architecture alignment",
                "goal": "Reach an attributable decision",
                "stage": "discussion",
                "creator": "project:subconscious"
            }
        }),
    ))
    .expect("create room");
    let room_id = created["room_id"].as_str().expect("room id");

    let _: Value = response(dispatch(
        &handler,
        json!({
            "op": "rooms.join",
            "params": {
                "room_id": room_id,
                "member_id": "project:uc-studio",
                "role": "colleague"
            }
        }),
    ))
    .expect("join colleague");

    let bound: Value = response(dispatch(
        &handler,
        json!({
            "op": "rooms.bind_member",
            "params": {
                "room_id": room_id,
                "member_id": "project:uc-studio",
                "project_id": "project:uc-studio",
                "session_id": "ses_uc_studio_1",
                "agent": "prometheus",
                "model": "openai/gpt-5",
                "delivery_mode": "background",
                "incarnation": 1
            }
        }),
    ))
    .expect("bind woken colleague");
    assert_eq!(bound["member"]["session_id"], "ses_uc_studio_1");
    assert_eq!(bound["member"]["incarnation"], 1);

    let stale = response::<Value>(dispatch(
        &handler,
        json!({
            "op": "rooms.post",
            "params": {
                "room_id": room_id,
                "author": "project:uc-studio",
                "incarnation": 0,
                "post_type": "position",
                "content": "ghost turn from the pre-wake actor"
            }
        }),
    ))
    .expect_err("stale actor must be fenced");
    assert!(stale.starts_with("stale_incarnation:"), "{stale}");

    let current: Value = response(dispatch(
        &handler,
        json!({
            "op": "rooms.post",
            "params": {
                "room_id": room_id,
                "author": "project:uc-studio",
                "incarnation": 1,
                "post_type": "position",
                "content": "current colleague turn"
            }
        }),
    ))
    .expect("current actor posts");
    assert_eq!(current["seq"], 1);

    let room: Value = response(dispatch(
        &handler,
        json!({ "op": "rooms.get", "params": { "room_id": room_id } }),
    ))
    .expect("get room");
    assert_eq!(room["posts"].as_array().expect("posts").len(), 1);
}
