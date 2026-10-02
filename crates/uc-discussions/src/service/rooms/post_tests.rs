use std::{cell::RefCell, path::PathBuf, rc::Rc};

use super::*;

thread_local! {
    static ADMITTED: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
}

pub(super) fn post_admitted() {
    let hook = ADMITTED.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

fn fixture() -> (PathBuf, RoomsService, RoomsService, String) {
    let directory = std::env::temp_dir().join(format!("discussions-post-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).expect("create isolated database directory");
    eprintln!("race database: {}", directory.display());
    let path = directory.join("rooms.sqlite");
    let service = RoomsService::new(Storage::open_path(&path).expect("open post connection"));
    let other = RoomsService::new(Storage::open_path(&path).expect("open competing connection"));
    other
        .storage
        .lock_connection()
        .expect("lock competitor")
        .busy_timeout(Duration::ZERO)
        .expect("disable timing-dependent busy retries");
    let room = service
        .create_room(CreateRoomRequest {
            topic: "Atomic admission".to_owned(),
            goal: "Serialize writers".to_owned(),
            stage: "discussion".to_owned(),
            creator: "alice".to_owned(),
        })
        .expect("create room");
    (directory, service, other, room.room_id)
}

fn request(room_id: &str, incarnation: Option<i64>) -> PostRoomRequest {
    PostRoomRequest {
        room_id: room_id.to_owned(),
        author: "alice".to_owned(),
        incarnation,
        post_type: "argument".to_owned(),
        content: "admitted post".to_owned(),
        reply_to_post_id: None,
    }
}

fn close(room_id: &str) -> CloseRoomRequest {
    CloseRoomRequest {
        room_id: room_id.to_owned(),
        decisions: Vec::new(),
        dissent: Vec::new(),
        outstanding_actions: Vec::new(),
    }
}

fn binding(room_id: &str, incarnation: i64) -> BindRoomMemberRequest {
    BindRoomMemberRequest {
        room_id: room_id.to_owned(),
        member_id: "alice".to_owned(),
        project_id: "project:alice".to_owned(),
        session_id: format!("session-{incarnation}"),
        agent: "Council: reviewer".to_owned(),
        model: "test-model".to_owned(),
        delivery_mode: "background".to_owned(),
        incarnation,
    }
}

fn assert_busy<T: std::fmt::Debug>(result: &Result<T, ServiceError>) {
    assert!(
        matches!(result,
        Err(ServiceError::Storage(crate::StorageError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.code == rusqlite::ErrorCode::DatabaseBusy),
        "competing write must be fenced by the immediate transaction: {result:?}"
    );
}

#[test]
fn post_close_is_serialized() {
    // Given two independent connections to the same WAL database.
    let (directory, service, other, room_id) = fixture();
    let competing_result = Rc::new(RefCell::new(None));
    let result_slot = competing_result.clone();
    let competitor = other.clone();
    let close_request = close(&room_id);
    ADMITTED.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            *result_slot.borrow_mut() = Some(competitor.close_room(close_request));
        }))
    });

    // When close is attempted exactly after admission, before insertion.
    let posted = service
        .post(request(&room_id, None))
        .expect("post commits first");

    // Then close cannot commit before the admitted post.
    let room = other
        .get_room(GetRoomRequest {
            room_id: room_id.clone(),
        })
        .expect("read state");
    assert_eq!(
        room.status, "active",
        "post committed after competing close"
    );
    assert_eq!(room.posts, vec![posted.post]);
    assert_eq!(posted.seq, 1);
    assert_busy(competing_result.borrow().as_ref().expect("close attempted"));
    other
        .close_room(close(&room_id))
        .expect("close after post commits");
    let before = other
        .get_room(GetRoomRequest {
            room_id: room_id.clone(),
        })
        .expect("closed state");
    assert!(matches!(
        service.post(request(&room_id, None)),
        Err(ServiceError::InvalidRequest(_))
    ));
    assert_eq!(
        other
            .get_room(GetRoomRequest { room_id })
            .expect("unchanged closed state"),
        before
    );
    drop(service);
    drop(other);
    std::fs::remove_dir_all(&directory).expect("remove successful race database");
    eprintln!("race database removed: {}", directory.display());
}

#[test]
fn post_rebind_is_serialized() {
    // Given an author bound at incarnation 1 and a separate writer.
    let (directory, service, other, room_id) = fixture();
    service
        .bind_member(binding(&room_id, 1))
        .expect("bind initial incarnation");
    let competing_result = Rc::new(RefCell::new(None));
    let result_slot = competing_result.clone();
    let competitor = other.clone();
    let rebind_request = binding(&room_id, 2);
    ADMITTED.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            *result_slot.borrow_mut() = Some(competitor.bind_member(rebind_request));
        }))
    });

    // When rebind is attempted exactly after admission, before insertion.
    let posted = service
        .post(request(&room_id, Some(1)))
        .expect("incarnation 1 commits first");

    // Then the incarnation cannot bump before this post commits.
    let room = other
        .get_room(GetRoomRequest {
            room_id: room_id.clone(),
        })
        .expect("read state");
    assert_eq!(
        room.members[0].incarnation,
        Some(1),
        "post committed after competing rebind"
    );
    assert_eq!(room.posts, vec![posted.post]);
    assert_eq!(posted.seq, 1);
    assert_busy(
        competing_result
            .borrow()
            .as_ref()
            .expect("rebind attempted"),
    );
    other
        .bind_member(binding(&room_id, 2))
        .expect("rebind after post commits");
    let before = other
        .get_room(GetRoomRequest {
            room_id: room_id.clone(),
        })
        .expect("rebound state");
    assert!(matches!(
        service.post(request(&room_id, Some(1))),
        Err(ServiceError::StaleIncarnation {
            expected: 2,
            received: Some(1),
            ..
        })
    ));
    assert_eq!(
        other
            .get_room(GetRoomRequest {
                room_id: room_id.clone()
            })
            .expect("unchanged rebound state"),
        before
    );
    assert_eq!(
        service
            .post(request(&room_id, Some(2)))
            .expect("current author posts")
            .seq,
        2
    );
    drop(service);
    drop(other);
    std::fs::remove_dir_all(&directory).expect("remove successful race database");
    eprintln!("race database removed: {}", directory.display());
}
