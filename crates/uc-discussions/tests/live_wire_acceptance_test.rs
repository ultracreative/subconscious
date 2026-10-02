#![forbid(unsafe_code)]

// allow: SIZE_OK - one ordered process-level lifecycle scenario and its wire fixture,
// kept in the single acceptance file required by the stabilization handoff.

use std::{
    fs::File,
    net::Ipv4Addr,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};

use serde_json::{json, Value};
use subc_daemon::{
    daemon_config::StorageConfig, read_frame, serve_listener, write_frame, ControlHandler, Frame,
    Registry, Router, ServerAuth,
};
use subc_protocol::{BindIdentity, Flags, FrameType, Priority, RouteTarget, PROTOCOL_VERSION};
use subc_test_support::TestTempDir;
use subc_transport::{
    authenticate_client, generate_daemon_id, generate_key, write_atomic, ConnectionInfo, Endpoint,
    SCHEMA_VERSION,
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::{sleep, timeout},
};

const DEADLINE: Duration = Duration::from_secs(10);

struct Harness {
    child: Child,
    daemon: JoinHandle<Result<(), subc_daemon::ServerError>>,
    _sandbox: TestTempDir,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.daemon.abort();
    }
}

async fn exchange(
    stream: &mut TcpStream,
    channel: u16,
    epoch: u32,
    corr: &mut u64,
    request: Value,
) -> (FrameType, Value) {
    *corr += 1;
    let frame = Frame::build(
        FrameType::Request,
        Flags::new(false, Priority::Interactive, false),
        channel,
        epoch,
        *corr,
        serde_json::to_vec(&request).expect("serialize request"),
    )
    .expect("request frame");
    timeout(DEADLINE, write_frame(stream, &frame))
        .await
        .expect("write deadline")
        .expect("write request");
    loop {
        let response = timeout(DEADLINE, read_frame(stream))
            .await
            .expect("response deadline")
            .expect("read response")
            .expect("connection alive");
        // Credit and control notifications can precede the correlated response.
        if matches!(response.header.ty, FrameType::Response | FrameType::Error)
            && response.header.corr == *corr
        {
            assert_eq!(response.header.channel, channel);
            assert_eq!(response.header.epoch, epoch);
            return (
                response.header.ty,
                serde_json::from_slice(&response.body).expect("JSON response"),
            );
        }
    }
}

async fn call(
    stream: &mut TcpStream,
    route: (u16, u32),
    corr: &mut u64,
    op: &str,
    params: Value,
) -> Value {
    let (ty, body) = exchange(
        stream,
        route.0,
        route.1,
        corr,
        json!({"op": op, "params": params}),
    )
    .await;
    assert_eq!(ty, FrameType::Response, "{op}: {body}");
    body
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_wire_room_lifecycle_is_fenced_and_preserved() {
    let sandbox = TestTempDir::new("uc-discussions-live-wire");
    let root = sandbox.path().to_path_buf();
    std::fs::create_dir_all(root.join("cortexkit/uc-discussions"))
        .expect("provision managed SQLite directory");
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener");
    let addr = listener.local_addr().expect("listener address");
    let conn = ConnectionInfo {
        schema: SCHEMA_VERSION,
        wire_version: Some(PROTOCOL_VERSION),
        endpoints: vec![Endpoint {
            host: addr.ip().to_string(),
            port: addr.port(),
        }],
        key: generate_key().expect("HMAC key"),
        daemon_id: generate_daemon_id().expect("daemon id"),
        pid: std::process::id(),
        daemon_ver: "live-wire-test".to_owned(),
    };
    let connection_file = root.join("subc-conn.json");
    write_atomic(&connection_file, &conn).expect("sandbox connection file");
    let registry = Arc::new(Registry::default());
    let control = Arc::new(
        ControlHandler::new(Arc::clone(&registry)).with_storage_config(Some(
            StorageConfig::Sqlite {
                data_home: root.clone(),
            },
        )),
    );
    let router = Arc::new(Router::with_control_handler(control));
    let daemon = tokio::spawn(serve_listener(
        listener,
        router,
        ServerAuth::new(conn.key.clone(), conn.daemon_id, conn.daemon_ver.clone()),
    ));
    let log = File::create(root.join("module.log")).expect("child log");
    let child = Command::new(env!("CARGO_BIN_EXE_ck-uc-discussions"))
        .args(["--subc", connection_file.to_str().expect("connection path")])
        .current_dir(&root)
        .env("HOME", &root)
        .env("XDG_DATA_HOME", &root)
        .env("XDG_CONFIG_HOME", &root)
        .stdout(Stdio::from(log.try_clone().expect("clone log")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn real module");
    let mut harness = Harness {
        child,
        daemon,
        _sandbox: sandbox,
    };
    let database = root.join("cortexkit/uc-discussions/store.db");
    timeout(DEADLINE, async {
        loop {
            assert!(
                harness.child.try_wait().expect("child status").is_none(),
                "module exited; see {}",
                root.join("module.log").display()
            );
            if registry
                .get_module("uc-discussions")
                .expect("registry")
                .is_some()
                && database.exists()
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("channel-0 HELLO/HELLO_ACK and sandbox storage initialization");
    println!(
        "PASS HELLO/HELLO_ACK registration and SQLite descriptor: {}",
        database.display()
    );

    let mut stream = TcpStream::connect(addr).await.expect("real TCP client");
    authenticate_client(&mut stream, &conn, DEADLINE)
        .await
        .expect("HMAC authentication");
    let mut corr = 0;
    let (ty, opened) = exchange(&mut stream, 0, 0, &mut corr, json!({
        "op": "route.open", "target": RouteTarget::ManagementSurface { module_id: "uc-discussions".to_owned() },
        "identity": BindIdentity::new(root.clone(), "live-wire-test", "acceptance")
    })).await;
    assert_eq!(ty, FrameType::Response, "route.open: {opened}");
    assert_eq!(opened["op"], "route.open");
    let route = (
        u16::try_from(opened["route_channel"].as_u64().expect("route channel"))
            .expect("u16 channel"),
        u32::try_from(opened["route_epoch"].as_u64().expect("route epoch")).expect("u32 epoch"),
    );
    assert_ne!(route.0, 0);
    println!("PASS authenticated channel-0 ManagementSurface route.open: {opened}");
    let created = call(&mut stream, route, &mut corr, "rooms.create", json!({"topic":"Local release", "goal":"Verify candidate", "stage":"discussion", "creator":"coordinator"})).await;
    let room = created["room_id"].as_str().expect("room id");
    let joined = call(
        &mut stream,
        route,
        &mut corr,
        "rooms.join",
        json!({"room_id":room, "member_id":"colleague", "role":"reviewer"}),
    )
    .await;
    assert_eq!(joined["ok"], true);
    let bound = call(&mut stream, route, &mut corr, "rooms.bind_member", json!({"room_id":room, "member_id":"colleague", "project_id":"test-project", "session_id":"test-session", "agent":"Council: Oracle", "model":"test-model", "delivery_mode":"background", "incarnation":1})).await;
    assert_eq!(bound["member"]["incarnation"], 1);
    let post = call(&mut stream, route, &mut corr, "rooms.post", json!({"room_id":room, "author":"colleague", "incarnation":1, "post_type":"proposal", "content":"Install locally only"})).await;
    assert_eq!(post["seq"], 1);
    let post_id = &post["post"]["post_id"];
    let before_stale = call(
        &mut stream,
        route,
        &mut corr,
        "rooms.get",
        json!({"room_id":room}),
    )
    .await;
    let (ty, stale) = exchange(&mut stream, route.0, route.1, &mut corr, json!({"op":"rooms.post", "params":{"room_id":room, "author":"colleague", "incarnation":0, "post_type":"position", "content":"ghost"}})).await;
    assert_eq!(ty, FrameType::Error);
    assert_eq!(stale["code"], "stale_incarnation");
    assert_eq!(
        call(
            &mut stream,
            route,
            &mut corr,
            "rooms.get",
            json!({"room_id":room})
        )
        .await,
        before_stale
    );
    println!("PASS stale incarnation rejected without timeline changes: {stale}");

    // Populate adjacent timeline surfaces, including a still-active poll.
    call(&mut stream, route, &mut corr, "rooms.object", json!({"room_id":room, "post_id":post_id, "author":"coordinator", "reason":"Keep external release on hold"})).await;
    call(&mut stream, route, &mut corr, "rooms.revise", json!({"room_id":room, "original_post_id":post_id, "author":"colleague", "diff_or_content":"Local-only with verified health"})).await;
    let poll = call(&mut stream, route, &mut corr, "rooms.poll", json!({"room_id":room, "question":"Install locally?", "options":["yes","no"], "created_by":"coordinator"})).await;
    let poll_id = &poll["poll"]["poll_id"];
    call(
        &mut stream,
        route,
        &mut corr,
        "rooms.vote",
        json!({"poll_id":poll_id, "voter":"colleague", "vote":"yes"}),
    )
    .await;
    call(
        &mut stream,
        route,
        &mut corr,
        "rooms.grant_stage",
        json!({"room_id":room, "grantee":"colleague", "granted_by":"coordinator", "ttl_ms":600000}),
    )
    .await;
    let active = call(
        &mut stream,
        route,
        &mut corr,
        "rooms.get",
        json!({"room_id":room}),
    )
    .await;
    let outcome = json!({"room_id":room, "decisions":["Install locally"], "dissent":["External release held"], "outstanding_actions":["Check supervisor health"]});
    let closed = call(
        &mut stream,
        route,
        &mut corr,
        "rooms.close",
        outcome.clone(),
    )
    .await;
    assert_eq!(closed["status"], "closed");
    chrono::DateTime::parse_from_rfc3339(closed["closed_at"].as_str().expect("closure timestamp"))
        .expect("valid committed closure timestamp");
    let frozen = call(
        &mut stream,
        route,
        &mut corr,
        "rooms.get",
        json!({"room_id":room}),
    )
    .await;
    assert_eq!(frozen["status"], "closed");
    assert_eq!(frozen["closed_at"], closed["closed_at"]);
    for field in ["posts", "members", "objections", "revisions", "polls"] {
        assert_eq!(frozen[field], active[field], "{field} preserved at closure");
    }
    for field in ["decisions", "dissent", "outstanding_actions"] {
        assert_eq!(frozen[field], outcome[field], "{field} preserved");
    }
    for (op, params, code) in [
        (
            "rooms.post",
            json!({"room_id":room, "author":"colleague", "incarnation":1, "post_type":"position", "content":"late"}),
            "invalid_request",
        ),
        (
            "rooms.object",
            json!({"room_id":room, "post_id":post_id, "author":"coordinator", "reason":"late"}),
            "invalid_request",
        ),
        (
            "rooms.revise",
            json!({"room_id":room, "original_post_id":post_id, "author":"colleague", "diff_or_content":"late"}),
            "invalid_request",
        ),
        (
            "rooms.grant_stage",
            json!({"room_id":room, "grantee":"coordinator", "granted_by":"coordinator", "ttl_ms":600000}),
            "invalid_request",
        ),
        (
            "rooms.vote",
            json!({"poll_id":poll_id, "voter":"colleague", "vote":"no"}),
            "invalid_request",
        ),
        (
            "rooms.vote",
            json!({"poll_id":poll_id, "voter":"coordinator", "vote":"no"}),
            "invalid_request",
        ),
        (
            "rooms.vote_poll",
            json!({"poll_id":poll_id, "voter":"colleague", "vote":"no"}),
            "unknown_operation",
        ),
        (
            "rooms.close",
            json!({"room_id":room, "decisions":["overwrite"], "dissent":[], "outstanding_actions":[]}),
            "invalid_request",
        ),
    ] {
        let (ty, rejected) = exchange(
            &mut stream,
            route.0,
            route.1,
            &mut corr,
            json!({"op":op, "params":params}),
        )
        .await;
        assert_eq!(ty, FrameType::Error, "{op}: {rejected}");
        assert_eq!(rejected["code"], code, "{op}: {rejected}");
        if code == "invalid_request" {
            assert_eq!(rejected["message"], format!("room {room} is not active"));
        }
        assert_eq!(
            call(
                &mut stream,
                route,
                &mut corr,
                "rooms.get",
                json!({"room_id":room})
            )
            .await,
            frozen,
            "{op} must not change complete room readback"
        );
        println!("PASS closed-room {op} rejected; identical timeline/outcome: {rejected}");
    }
    let persisted = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("sandbox database readback");
    let status: String = persisted
        .query_row(
            "SELECT status FROM rooms WHERE room_id = ?1",
            [room],
            |row| row.get(0),
        )
        .expect("wire-created room persisted in HELLO_ACK database");
    assert_eq!(status, "closed");
    drop(persisted);
    drop(stream);
    harness.child.kill().expect("stop module");
    harness.child.wait().expect("reap module");
    harness.daemon.abort();
    assert!((&mut harness.daemon)
        .await
        .as_ref()
        .is_err_and(|error| error.is_cancelled()));
    drop(harness);
    assert!(!root.exists(), "sandbox removed");
    assert!(TcpStream::connect(addr).await.is_err(), "listener released");
    println!(
        "TEARDOWN child reaped, sockets closed, sandbox removed: {}",
        root.display()
    );
}
