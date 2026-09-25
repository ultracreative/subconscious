# UC Discussions: Deployment and Operations Runbook

This document covers operational procedures, architecture, deployment options, client usage, and verification evidence for the `uc-discussions` service (`ck-uc-discussions`).

---

## 1. Overview and Core Invariants

The `uc-discussions` service provides CortexKit agent environments with durable direct messaging (`peer.*`), structured multi-party deliberation rooms (`rooms.*`), and multi-model Athena council coordination (`council.*`). It runs as a supervised module under the `ck-subc` daemon, communicating across authenticated loopback transport and serving typed operations via the `ManagementSurface` protocol.

```
+-------------------------------------------------------------+
|                  Agent Clients / UCS Host                   |
|   (SubcDiscussionsAdapter, project_message, athena_council) |
+------------------------------+------------------------------+
                               | Subc Wire Protocol (TCP)
                               v
+-------------------------------------------------------------+
|                         ck-subc                             |
|          Per-User Daemon & Opaque Message Router            |
+------------------------------+------------------------------+
                               | Channel 0 / Authenticated Loopback
                               v
+-------------------------------------------------------------+
|                     ck-uc-discussions                       |
|           Supervised Service (ManagementSurface)            |
|  +-----------------+ +-------------------+ +--------------+  |
|  |   peer.* (DM)   | |  rooms.* (Rooms)  | | council.*    |  |
|  +--------+--------+ +---------+---------+ +-------+------+  |
|           |                    |                   |         |
|           +--------------------+-------------------+         |
|                                |                             |
|                                v                             |
|                      SQLite Engine (WAL)                     |
|                 ~/.local/share/cortexkit/                    |
|                   uc-discussions/store.db                    |
+-------------------------------------------------------------+
```

### Core Invariants

1. **Opaque Daemon Routing**: The `ck-subc` daemon never inspects, deserializes, or rewrites discussion message payloads. It inspects only the 21-byte binary header to route frames to the module. The `uc-discussions` module owns all schema interpretation, payload validation, and business logic.
2. **Single-Writer SQLite Durability**: All storage access goes through a managed SQLite instance configured with write-ahead logging (`journal_mode = WAL`), a 5000 ms busy timeout, and enforced foreign keys. State mutations execute inside immediate transactions (`BEGIN IMMEDIATE`) so sequence numbers, room grants, and lease acquisitions serialize safely.
3. **Monotonic Sequence Allocation**: Deliberation room posts receive strictly ascending sequence numbers (`seq = 1, 2, 3...`) unique within each room. Sequence gaps, reorderings, and backdated insertions are rejected.
4. **Attributable Deliberation Closure**: Deliberation rooms cannot simply stop or emit ungrounded summaries. Closing a room requires explicit arrays of agreed decisions, recorded dissent or objections, and concrete outstanding action items with identified owners.
5. **All-Terminal Council Convergence**: An Athena council cannot reconcile until every declared member reaches an explicit terminal status (`completed`, `failed`, or `cancelled`). If any member is still running, staged, or missing, `council.reconcile` rejects the request with an actionable diagnostic identifying the blocking member.
6. **Explicit Three-State Delivery Receipts**: Peer messages advance through three discrete states: `pending`, `delivered`, and `processed`. Receipts write timestamps and session IDs atomically. Polling cursors rely on monotonic ordering and state filtering, preventing duplicate prompt injection when an agent reconnects or recovers from a crash.
7. **Transparent Courier Fallback**: The host messaging layer (`project_message`) routes through the `uc-discussions` fast path first. If the daemon or module is offline, unreachable, or returns a transport error, the courier falls back immediately to the local file mailbox protocol without losing the message or throwing an unhandled exception to the agent.

---

## 2. How to Deploy

The `uc-discussions` service ships as a compiled Rust binary named `ck-uc-discussions`. Operators can run it as an optional managed component within UC Studio (UCS), supervise it through platform init systems, or invoke the standalone binary directly.

### Binary Information

- **Canonical Path**: `/Volumes/Topper2TB/.cargo-target/release/ck-uc-discussions`
- **Architecture**: Mach-O 64-bit executable arm64 (macOS Apple Silicon)
- **Codesigning**: Hardened runtime enabled (`flags=0x10000(runtime)`), signed under designated authority `Arcus Local Dev`.

### Method A: UC Studio Configuration Toggle

Within UC Studio, `uc-discussions` is registered as an optional bundled daemon. It stays disabled by default so minimal setups don't spawn idle services.

To enable the service in your project or user profile, edit your configuration file (such as `~/.config/ucs/profile.json` or `.ucs/profile.json`):

```json
{
  "components": {
    "discussions_daemon": {
      "enabled": true
    }
  }
}
```

When `components.discussions_daemon.enabled` is `true`:
- The UCS setup lane extracts and verifies the binary against the Arcus pin.
- The supervisor unit starts on login or workstation boot.
- The UCS runtime bridge wires `SubcDiscussionsAdapter` to outgoing courier calls.

Setting the field to `false` disables startup. In-flight calls finish cleanly, the module sends a `Goodbye` frame on channel 0, releases open leases, and shuts down without corrupting SQLite state.

### Method B: macOS LaunchAgent

For persistent workstation operation on macOS, install a LaunchAgent plist in the user domain. This runs without root privileges and starts on graphical user login.

1. Create `~/Library/LaunchAgents/com.cortexkit.uc-discussions.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.cortexkit.uc-discussions</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Volumes/Topper2TB/.cargo-target/release/ck-uc-discussions</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>/Users/brethoffman/.local/share/cortexkit/uc-discussions/stdout.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/brethoffman/.local/share/cortexkit/uc-discussions/stderr.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info,uc_discussions=debug</string>
    </dict>
</dict>
</plist>
```

2. Load and start the service:

```bash
# Ensure log directory exists
mkdir -p ~/.local/share/cortexkit/uc-discussions

# Load and launch
launchctl load -w ~/Library/LaunchAgents/com.cortexkit.uc-discussions.plist

# Check status
launchctl list | grep uc-discussions
```

3. To stop and unload:

```bash
launchctl unload -w ~/Library/LaunchAgents/com.cortexkit.uc-discussions.plist
```

### Method C: Linux systemd User Service

On Linux or Steam Deck systems, deploy the daemon using a systemd user unit. This needs zero root permissions and keeps state isolated in user space.

1. Create `~/.config/systemd/user/ck-uc-discussions.service`:

```ini
[Unit]
Description=CortexKit UC Discussions Service
After=network.target

[Service]
Type=simple
ExecStart=/Volumes/Topper2TB/.cargo-target/release/ck-uc-discussions
Restart=on-failure
RestartSec=3s
Environment=RUST_LOG=info,uc_discussions=debug
StandardOutput=append:%h/.local/share/cortexkit/uc-discussions/stdout.log
StandardError=append:%h/.local/share/cortexkit/uc-discussions/stderr.log

[Install]
WantedBy=default.target
```

2. Enable and start:

```bash
mkdir -p ~/.local/share/cortexkit/uc-discussions
systemctl --user daemon-reload
systemctl --user enable --now ck-uc-discussions.service
systemctl --user status ck-uc-discussions.service
```

### Method D: Standalone Binary Execution

For debugging, performance benchmarking, or test environments, you can run the binary directly in a terminal:

```bash
RUST_LOG=debug /Volumes/Topper2TB/.cargo-target/release/ck-uc-discussions
```

The process reads daemon connection tokens from the standard CortexKit connection file (`~/.local/share/cortexkit/subc.json` or `$XDG_DATA_HOME/cortexkit/subc.json`), registers its manifest on channel 0, and begins accepting queries and mutations.

---

## 3. How to Use

### Direct TypeScript Client (`SubcDiscussionsAdapter`)

The primary programmatic interface lives in `@oh-my-opencode/omo-opencode/features/discussions`. Import `SubcDiscussionsAdapter` to interact directly with the discussion engine.

```typescript
import { SubcDiscussionsAdapter } from "@oh-my-opencode/omo-opencode/features/discussions"

// Connect using the active Subc client transport
const discussions = new SubcDiscussionsAdapter({
  transport: subcClientTransport,
  module_id: "uc-discussions",
})
```

#### 1. Peer Messaging (Direct Agent DMs)

Send direct messages between agent sessions with delivery tracking and cursored inbox queries.

```typescript
// Enqueue an urgent question to another agent
const sent = await discussions.enqueueMessage({
  fromName: "code-architect",
  toName: "security-auditor",
  session_id: "ses_sec_99182",
  body: "Does the token verification bypass allow unauthenticated replay?",
  intent: "question",
  priority: 10,
})
console.log("Enqueued message ID:", sent.message_id)

// Target agent polls inbox for new items
const inbox = await discussions.pollInbox({
  session_id: "ses_sec_99182",
  limit: 20,
})

for (const msg of inbox.messages) {
  // Acknowledge receipt upon receipt
  await discussions.ackMessage({
    message_id: msg.message_id,
    session_id: "ses_sec_99182",
    receipt_type: "delivery",
  })

  // Process work and mark finished
  handleSecurityReview(msg.body)

  await discussions.ackMessage({
    message_id: msg.message_id,
    session_id: "ses_sec_99182",
    receipt_type: "processing",
  })
}
```

#### 2. Structured Deliberation Rooms

Deliberation rooms let multiple agents converse, contest proposals, vote, and lock in attributable closure.

```typescript
// 1. Create a deliberation room
const room = await discussions.createRoom({
  topic: "Database Migration v2",
  goal: "Decide whether to migrate SQLite tables in-place or write to a fresh database",
  stage: "deliberation",
  creator: "coordinator",
})
const roomId = room.room_id

// 2. Agents join the room with assigned roles
await discussions.joinRoom({ roomId, member_id: "agent-lead", role: "proposer" })
await discussions.joinRoom({ roomId, member_id: "agent-dba", role: "reviewer" })
await discussions.joinRoom({ roomId, member_id: "agent-qa", role: "participant" })

// 3. Proposer posts an initial proposal
const post1 = await discussions.postRoom({
  roomId,
  author: "agent-lead",
  post_type: "proposal",
  content: "Migrate tables in-place with an exclusive transaction lock.",
})

// 4. Reviewer registers a formal objection against post 1
await discussions.objectRoom({
  roomId,
  post_id: post1.post.post_id,
  author: "agent-dba",
  reason: "In-place migration risks corruption if storage fills during write-ahead logging.",
})

// 5. Proposer posts a revised approach addressing the objection
await discussions.reviseRoom({
  roomId,
  original_post_id: post1.post.post_id,
  author: "agent-lead",
  diff_or_content: "Write a fresh temporary database, verify checksums, then swap file descriptors.",
})

// 6. Close the room with full attribution
await discussions.closeRoom({
  roomId,
  decisions: [
    "Write new database copy first",
    "Swap file descriptors only after integrity check passes",
  ],
  dissent: [
    "Agent DBA noted this requires 2x temporary disk headroom",
  ],
  outstanding_actions: [
    "agent-qa: verify disk headroom check before swap executes",
  ],
})
```

### Cross-Project Messaging via `project_message`

Agents communicate across project boundaries using the `project_message` tool. The implementation in `packages/omo-opencode/src/features/cross-project-mailbox/send-tool/project-message-tool.ts` uses `SubcDiscussionsAdapter` as a fast delivery path while keeping file mailboxes as a fallback.

```
runProjectMessageSend(input, deps)
                  |
                  v
       deps.discussionsAdapter present?
        /                           \
      YES                            NO
       |                              |
       v                              |
Try discussions.enqueueMessage(...)   |
       |                              |
    Success?                          |
    /      \                          |
  YES       NO (thrown error)         |
   |         \--------------------+   |
   v                              v   v
Outbox log updated          Fall back to file mailbox
Detail: "subc-fast-path"    Write .ucs/coordination_notes/
Return ok                   Return ok
```

The tool checks if the adapter is available and calls `enqueueMessage`. If successful, the send trace records `detail: "subc-fast-path"` and exits.

If `uc-discussions` is not running or throws any error, the adapter catches the exception silently. Execution drops through to the standard file-based courier (`writeNote`), placing the message into `.ucs/coordination_notes/<target>/pending/`. Agents never experience a failed send because the daemon was temporarily stopped.

### Athena Council Integration

Athena orchestrates multi-agent model evaluation panels using the `council.*` operations. The flow guarantees that all assigned members finish work before a consensus statement is accepted.

1. **Stage Run (`council.stage`)**: Athena initializes a council run with a unique ID, evaluation prompt, and declared participant names.
2. **Evaluate Members (`council.evaluate`)**: As each model completes its prompt turn, it sends its status (`completed`, `failed`, or `cancelled`) and response text. The service tracks aggregate progress and returns `all_members_terminal: boolean`.
3. **Reconcile Deliberation (`council.reconcile`)**: Athena posts the final consensus synthesis and agreement level. The database transaction verifies that all declared members reached a terminal status. If any member is still running or staged, reconciliation aborts immediately.
4. **Deliberation Archiving**: The transcript, individual member outputs, and consensus verdict are written to `.omo/athena/council-<slug>/` for permanent review.

---

## 4. Monitoring and Diagnostics

### Health Reporting via `ck health`

The `ck` operator utility queries module health over channel 0.

To check the operational status of `uc-discussions`:

```bash
ck health uc-discussions
```

Expected standard output:
```text
uc-discussions  ok  uc-discussions operational
```

For automated monitoring pipelines, request JSON output:

```bash
ck --json health uc-discussions
```

Expected response format:
```json
{
  "module_id": "uc-discussions",
  "status": "ok",
  "detail": "uc-discussions operational",
  "metrics": null
}
```

If the SQLite database fails to open or initialize migrations during startup, the service sets its status to `degraded` and includes the root cause message in the `detail` property.

### SQLite Database Inspection

The service maintains a single SQLite database in the platform data directory.

- **Standard Linux / macOS Path**:
  `$HOME/.local/share/cortexkit/uc-discussions/store.db`
- **Custom XDG Path** (if `$XDG_DATA_HOME` is set):
  `$XDG_DATA_HOME/cortexkit/uc-discussions/store.db`
- **Auxiliary Files**:
  `store.db-wal` (write-ahead log) and `store.db-shm` (shared memory index)

#### Database Schema Tables

| Table Name | Description | Key Indexes / Constraints |
|---|---|---|
| `leases` | Distributed resource leases | `resource_id` PRIMARY KEY |
| `peer_threads` | Conversation threads for direct messages | `thread_id` PRIMARY KEY |
| `peer_messages` | Message payload, state, receipts | `message_id` PRIMARY KEY |
| `rooms` | Deliberation rooms and goals | `room_id` PRIMARY KEY |
| `room_members` | Room membership and roles | PRIMARY KEY (`room_id`, `member_id`) |
| `room_posts` | Room statements, proposals, replies | UNIQUE (`room_id`, `seq`) |
| `room_objections` | Formal objections tied to posts | `objection_id` PRIMARY KEY |
| `room_revisions` | Revisions to original posts | `revision_id` PRIMARY KEY |
| `room_polls` | Deliberation polls and option lists | `poll_id` PRIMARY KEY |
| `room_votes` | Member ballot entries | PRIMARY KEY (`poll_id`, `voter`) |
| `room_stage_grants` | Active speaking floor leases | `grant_id` PRIMARY KEY |
| `council_runs` | Athena council deliberation sessions | `council_id` PRIMARY KEY |
| `council_member_states` | Status and output per council member | PRIMARY KEY (`council_id`, `member_name`) |
| `schema_migrations` | Applied database migration versions | `version` PRIMARY KEY |

#### Useful Diagnostic Queries

Run these queries with the `sqlite3` CLI against the database file:

```bash
DB="$HOME/.local/share/cortexkit/uc-discussions/store.db"

# 1. Count pending vs delivered vs processed peer messages
sqlite3 "$DB" "SELECT state, count(*) FROM peer_messages GROUP BY state;"

# 2. Check for active unclosed deliberation rooms
sqlite3 "$DB" "SELECT room_id, topic, stage, status, created_at FROM rooms WHERE status != 'closed';"

# 3. Check for open, unresolved objections in a specific room
sqlite3 "$DB" "SELECT objection_id, post_id, author, reason FROM room_objections WHERE status = 'open';"

# 4. View active, unexpired leases
sqlite3 "$DB" "SELECT resource_id, holder_id, expires_at FROM leases WHERE expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now');"

# 5. Check council runs that have not finished
sqlite3 "$DB" "SELECT council_id, name, status, started_at FROM council_runs WHERE status != 'completed';"

# 6. Check stalled council members blocking reconciliation
sqlite3 "$DB" "SELECT council_id, member_name, status, error_text FROM council_member_states WHERE status NOT IN ('completed', 'failed', 'cancelled');"
```

### Arcus Release Validation

The package envelope must conform to Arcus distribution specifications before publication. Run the official validator script from the `uc-studio` toolchain:

```bash
ENVELOPE="/Volumes/Topper2TB/Git/uc-studio/manifests/v2/uc-discussions/releases/uc-discussions-0.1.0.json"
TOOLCHAIN="/Volumes/Topper2TB/Git/uc-studio/packages/arcus/toolchain/scripts/validate-arcus.sh"

sh "$TOOLCHAIN" --body-only "$ENVELOPE"
```

The validator confirms:
1. Strict inequality across the distinct digest triples (`archive_sha256 != target_content_source.sha256 != tree_signature.sha256`).
2. Exact matching between declared archive sizes and compressed assets.
3. Clean target specification for `darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`, and `windows-x64`.

---

## 5. Verification Evidence

The implementation and operational behavior are validated through automated regression suites and real inter-project courier notes.

### Multi-Turn Deliberation Room Test (`acceptance_test.rs`)

Location: `crates/uc-discussions/tests/acceptance_test.rs`

This test exercises a complete 10-step multi-turn deliberation:
1. Coordinator creates a room: `Release Strategy`, goal `Decide 0.1.0 release policy`.
2. Three agents join: `agent-a` (proposer), `agent-b` (reviewer), `agent-c` (participant).
3. `agent-a` submits post 1 with initial proposal: `"Ship globally immediately"`.
4. `agent-b` registers a formal objection against post 1: `"Rollback canary metrics are not configured"`.
5. `agent-a` submits a revision linked to post 1: `"Ship 10% canary with automated error rollback threshold"`.
6. `agent-c` creates a poll: `"Approve revised canary plan?"` with options `["yes", "no"]`.
7. `agent-a` and `agent-b` cast affirmative votes.
8. Coordinator grants speaking floor stage to `agent-a` with a TTL grant.
9. Room closes with attributable decisions, recorded dissent, and outstanding actions.
10. Querying `rooms.get` verifies:
    - Status is `closed` with valid timestamp.
    - All posts have strictly monotonic sequence numbers (`seq = 1, ...`).
    - Objections and revisions are correctly attached to post 1.
    - Member roles match registration (`proposer`, `reviewer`, `participant`).

### Chaos and Restart Persistence Proof (`chaos_recovery_test.rs`)

Location: `crates/uc-discussions/tests/chaos_recovery_test.rs`

This test proves zero state loss and zero duplicate prompt injection across abrupt crashes:
- **Phase 1 (Pre-Crash)**: Instance 1 opens storage, enqueues message 1 and message 2, marks message 1 as `delivered` and `processed`, creates a room, posts a proposal (`seq = 1`), and acquires a short lease (50 ms TTL).
- **Phase 2 (Simulated Crash)**: Handler and storage are dropped abruptly without shutdown notifications. The thread sleeps 70 ms so the active lease expires.
- **Phase 3 (Post-Restart Recovery)**: Instance 2 opens the same SQLite file.
  - **No Duplicate Prompt Injection**: Polling the inbox for `agent-x` with `after_id = msg1_id` returns only `msg-2`. The processed `msg-1` is never returned.
  - **Receipt Durability**: Direct inspection of the database proves `msg-1` remains `processed` with both `delivery_receipt` and `processing_receipt` intact.
  - **Room State Preservation**: Room status, membership records, and post sequence numbers match the pre-crash state exactly.
  - **Expired Lease Re-acquisition**: The expired lease on `lock-1` is acquired immediately by a new worker, confirming automatic TTL cleanup.

### Zero-Loss Export and Import Snapshot Proof (`migration_snapshot_test.rs`)

Location: `crates/uc-discussions/tests/migration_snapshot_test.rs`

This test verifies complete portability and migration safety:
- Populates database 1 with rows across all 12 tables: leases, peer threads, peer messages in diverse states, rooms, members, posts, objections, revisions, polls, votes, stage grants, council runs, and member states.
- Generates a `DiscussionsSnapshot` structure via `Storage::export_snapshot`.
- Instantiates a clean, empty database 2 and applies the snapshot via `Storage::import_snapshot`.
- Queries both databases and asserts 100% field parity: identical counts, identical timestamps, identical JSON outcomes, and preserved foreign relationships.

### Real Multi-Project Courier Notes

The fleet verified cross-project messaging and consensus using courier notes preserved in `.ucs/coordination_notes/`:
- **`lore-300dc729` -> `subconscious-770c36a2`** (`ee43b818-eb2e-47b7-b2a5-bb03b3a5d95b.md`): Confirmed module ownership boundaries and verified that subconscious owns the daemon/protocol contracts while keeping `uc-discussions` independent of untracked placeholders.
- **`uc-studio-8ce41322` -> `subconscious-770c36a2`** (`9ec1ec30-254a-4054-8e77-b3c0eb4792c6.md`): Authoritative architectural confirmation detailing user-space staging paths, LaunchAgent and systemd specifications, `components.discussions_daemon.enabled` configuration schema, and the permanent mailbox fallback boundary.
- **`subconscious-770c36a2` Outbox Logs** (`.ucs/mailbox-outbox.jsonl`): Records successful exchanges with `uc-studio` and `lore`, demonstrating trace collection and verifiable courier delivery.
