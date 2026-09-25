#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;

use uc_discussions::protocol::rooms::{
    CreateRoomRequest, GetRoomRequest, JoinRoomRequest, PostRoomRequest,
};
use uc_discussions::service::rooms::RoomsService;
use uc_discussions::Storage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home =
        env::var("HOME").map_err(|e| format!("failed to read HOME environment variable: {e}"))?;
    let db_path = PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("cortexkit")
        .join("uc-discussions")
        .join("store.db");

    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let storage = Storage::open_path(&db_path)?;
    let rooms_service = RoomsService::new(storage);

    let create_resp = rooms_service.create_room(CreateRoomRequest {
        topic: "Fleet Fork-Sync & Arcus Blessed Rollout".to_string(),
        goal: "Complete the latest fork-sync on uc-studio, publish release to Arcus and ucs-setup, update blessed set to current latest releases, and apply/install to local machine, cloudhome services, and workstations.".to_string(),
        stage: "deliberation".to_string(),
        creator: "project:subconscious".to_string(),
    })?;

    let members = [
        "project:uc-studio",
        "project:cloudhome",
        "project:magic-context",
    ];
    for member_id in members {
        rooms_service.join_room(JoinRoomRequest {
            room_id: create_resp.room_id.clone(),
            member_id: member_id.to_string(),
            role: "participant".to_string(),
        })?;
    }

    let proposal = "Roadmap for fleet synchronization:\n\
1. uc-studio: Complete latest fork-sync from upstream and build release artifacts.\n\
2. uc-studio / arcus: Publish and sign release bundle to Arcus v2/v3 and ucs-setup.\n\
3. uc-studio: Update embedded-blessed-set.json and arcus-blessed-plugins.json to all latest component releases.\n\
4. Fleet Rollout: Run ucs-setup apply/install across local seat, cloudhome cluster services, and connected workstations.";

    let post_resp = rooms_service.post(PostRoomRequest {
        room_id: create_resp.room_id.clone(),
        author: "project:subconscious".to_string(),
        incarnation: None,
        post_type: "proposal".to_string(),
        content: proposal.to_string(),
        reply_to_post_id: None,
    })?;

    assert_eq!(
        post_resp.seq, 1,
        "initial proposal should have sequence 1, got {}",
        post_resp.seq
    );

    let room = rooms_service.get_room(GetRoomRequest {
        room_id: create_resp.room_id,
    })?;

    let room_json = serde_json::to_string_pretty(&room)?;
    println!("{room_json}");

    Ok(())
}
