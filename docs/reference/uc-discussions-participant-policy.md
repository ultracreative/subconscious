# UC Discussions Participant Policy & Enforcement Boundary

This specification defines the participant policy, colleague declaration contract, and host-dependent enforcement boundary for multi-agent deliberation rooms in `uc-discussions`.

---

## 1. Context and Problem Statement

When multiple AI agents deliberate within a shared repository or workspace, uncoordinated file modifications pose severe operational risks:
- **Write Collisions**: Concurrent agents attempting to edit identical files or working trees risk corrupting code, producing merge conflicts, and invalidating test runs.
- **Premature Implementation**: Deliberation and consensus rooms (such as architectural debate, design critique, and pre-production phase gates) are intended solely for analytical evaluation, dissent, revision, and consensus building—not direct workspace mutation.
- **Attribution Confusion**: When mutating agents participate in deliberation channels under ambiguous names, workspace mutations cannot be reliably audited back to specific deliberation decisions.

To protect workspaces from uncoordinated file mutations, deliberation rooms in `uc-discussions` are architecturally intended for **read-only Council agents**. However, an architectural policy cannot rely on informal convention or superficial name matching alone. This document details the exact policy contract, the daemon-side state validation guard, the architectural limits of daemon-side enforcement, and the external host-owner contract required for genuine safety.

---

## 2. Colleague Declaration Contract

### 2.1 Colleague Scope

A **colleague** is a peer agent participant bound to a deliberation room via `rooms.bind_member` (`BindRoomMemberRequest`).

Colleague declarations are strictly restricted to:
```
Council: <non-empty member name>
```

### 2.2 Exact String & Pattern Validation

The member agent declaration string must satisfy the following specification:
- Must begin with the exact case-sensitive prefix: `"Council: "` (ASCII `'C'`, `'o'`, `'u'`, `'n'`, `'c'`, `'i'`, `'l'`, `':'`, `' '`).
- The remainder of the string after the prefix must be non-empty after trimming leading and trailing ASCII whitespace.
- Formally, the declaration must conform to the regular expression:
  ```regex
  ^Council: \S.*$
  ```
  Or equivalently in string manipulation logic:
  ```rust
  let trimmed = agent.strip_prefix("Council: ").map(str::trim);
  matches!(trimmed, Some(name) if !name.is_empty())
  ```

### 2.3 Explicit Treatment of Agent Names

| Agent String | Status | Rationale |
|---|---|---|
| `Council: claude-sonnet` | **Accepted** | Conforms to `Council: <name>` contract. |
| `Council: momus` | **Accepted** | Conforms to `Council: <name>` contract. |
| `Council: gpt-4o` | **Accepted** | Conforms to `Council: <name>` contract. |
| `Council: athena` | **Accepted** | Conforms to `Council: <name>` contract. |
| `Council: metis` | **Accepted** | Conforms to `Council: <name>` contract. |
| `Athena` | **Rejected** | Bare analytical agent name; lacks explicit Council prefix. |
| `Athena-Junior` | **Rejected** | Bare analytical agent name; lacks explicit Council prefix. |
| `Oracle` | **Rejected** | Bare analytical agent name; lacks explicit Council prefix. |
| `Momus` | **Rejected** | Bare analytical agent name; lacks explicit Council prefix. |
| `Metis` | **Rejected** | Bare analytical agent name; lacks explicit Council prefix. |
| `Council:` | **Rejected** | Missing member name. |
| `Council: ` | **Rejected** | Suffix contains only whitespace. |
| `council: claude-sonnet` | **Rejected** | Case mismatch (`council` instead of `Council`). |
| `hephaestus` | **Rejected** | Mutating build/code agent. |
| `build` | **Rejected** | Mutating build agent. |
| `Sisyphus-Junior` | **Rejected** | Mutating executor agent. |
| `Council: hephaestus` | **Allowed by Syntax / Disallowed by Host Contract** | Conforms to syntax pattern, but host owner contract must refuse mutating profiles (see Section 5). |

**Crucial Policy Invariant**:
Agents such as `Athena`, `Athena-Junior`, `Oracle`, `Momus`, and `Metis` are well-known analytical and deliberation personas across the fleet. However, they are **NOT automatically treated as Council members** when declared as bare names. To participate as a bound colleague in a deliberation room, they must be explicitly declared under the council prefix (e.g. `Council: momus`, `Council: athena`).

Mutating build/executor agents (such as `hephaestus`, `build`, `Sisyphus-Junior`, `coder`, or similar implementation engines) are strictly prohibited from colleague binding.

---

## 3. Unbound Initiator & Coordinator Boundary

Deliberation rooms involve two distinct categories of participants:
1. **Room Creator & Unbound Coordinator**:
   - The party initiating the deliberation creates the room via `rooms.create` (`CreateRoomRequest.creator`).
   - The coordinator may submit contextual overview turns, agendas, or stage grants using `rooms.post` (`PostRoomRequest.author`) prior to or without binding an active spawned session.
   - The unbound initiator/coordinator is **not a spawned colleague** and is not registered through `rooms.bind_member`.
   - Therefore, the creator and unbound author identifiers remain outside the `Council: <name>` colleague declaration check.
2. **Spawned Colleague Members**:
   - Colleagues spawned across registered fleet projects to deliberate, object, revise, and vote on proposals.
   - Colleagues are bound to the room via `rooms.bind_member`, which associates their room `member_id` with an explicit `project_id`, `session_id`, `agent`, `model`, `delivery_mode`, and `incarnation`.
   - All spawned colleagues are subject to the `Council: <name>` declaration check.

---

## 4. Daemon State vs. Host Sandbox Boundary

A fundamental principle of the CortexKit architecture is that **provenance and safety guarantees are attested only by the component capable of enforcing them**.

### 4.1 What the Daemon Enforces

The `uc-discussions` daemon (`ck-uc-discussions`) maintains stateful deliberation records in managed SQLite storage. In `RoomsService::bind_member`, the daemon verifies:
1. Presence of required fields: `room_id`, `member_id`, `project_id`, `session_id`, `agent`, `model`, and `delivery_mode`.
2. Incarnation fencing: monotonic positive integer increment (`incarnation > current_incarnation`).
3. Room state: the room must be in `active` state (mutations to closed rooms are rejected).
4. **Planned Colleague Policy Guard**: Verification that `agent` matches `^Council: \S.*$`.

### 4.2 What the Daemon CANNOT Enforce

It is **architecturally impossible** for the daemon to enforce filesystem safety or workspace immutability through a name string check:
- **No Process Authority**: The daemon does not spawn participant processes, does not execute agent tool calls, and does not control the operating system environment.
- **No Identity Attestation**: In the request payload `{"agent": "Council: claude-sonnet"}`, the `agent` field is a self-declared string provided by the caller. The daemon-side check cannot verify whether the executing process is genuinely `claude-sonnet` or a completely different binary.
- **No Sandbox Control**: The daemon cannot intercept system calls, cannot restrict disk writes, and cannot revoke mutating tools from an agent's runtime context.
- **Transport Disconnect**: While the client connection carries an authenticated route handle (`RequestCtx`, `RouteBindRequest`) via loopback TCP and HMAC, the daemon routes wire messages; it does not sandbox the host runner that initiated the connection.

> **CRITICAL ARCHITECTURAL WARNING**:
> A daemon-side check for `Council: <name>` is an **admission filter on recorded metadata**, NOT an execution sandbox. Claiming that a daemon string check prevents filesystem mutations or file collisions is false. Actual safety depends entirely on host capability gating.

---

## 5. Required Host Owner Contract (External Hold)

Because execution takes place on the host system (e.g. via the host plugin, `project_room` orchestrator, or agent harness), read-only safety requires an explicit, audited **Host Owner Contract**.

### 5.1 Host Responsibilities

The host environment orchestrating room deliberations must guarantee three core invariants:

1. **Trusted Binding Authority**:
   - Only the host deliberation coordinator is permitted to invoke `rooms.bind_member`.
   - The host must populate `project_id`, `session_id`, `agent`, `model`, and `delivery_mode` truthfully based on verified execution configuration.

2. **Read-Only Agent Profile Selection**:
   - When launching or dispatching to a deliberation colleague, the host must select an agent profile that is provably read-only.
   - The host must explicitly strip or deny mutating tools from the agent's tool catalog, specifically:
     - File modification tools: `write`, `edit`, `patch`, `file_writer`, `apply_patch`.
     - System execution tools with write side-effects: arbitrary shell execution (`bash`), compiler mutations, or git mutation commands (`git commit`, `git checkout`, `git push`).
   - The agent profile should retain only analytical tools: repository inspection (`read`, `aft_search`, `aft_zoom`, `aft_outline`, `aft_inspect`), documentation queries, and room interaction APIs (`rooms.post`, `rooms.object`, `rooms.vote`).

3. **Effective Tool Permission Enforcement**:
   - Host enforcement must be enforced at the tool dispatcher / sandbox boundary (e.g., capability whitelisting in OpenCode / Claude Code / Codex harnesses), not via prompt instructions or system prompts alone.
   - An agent prompt instructing an LLM "do not edit files" is an advisory heuristic, not an enforcement mechanism. The tool dispatcher must actively reject or withhold mutating tool definitions.

### 5.2 External Hold Status

**Host-dependent sandbox enforcement is an EXTERNAL HOLD.**
- The `uc-discussions` service repository cannot configure, modify, or verify the host agent runner's tool sandboxing.
- Implementation of the daemon-side `Council: <name>` guard in `uc-discussions` satisfies only the local state and schema contract.
- The deliberation system must not be advertised as "collision-safe" or "guaranteed read-only" until the host owner contract is formalized, implemented in the host runner (`project_room`), and verified with empirical failure-mode tests (attempting forbidden writes).

---

## 6. Future Bounded Guard at `RoomsService::bind_member`

### 6.1 Code Seam & Validation Logic

The daemon-side guard will be located directly in `crates/uc-discussions/src/service/rooms.rs` inside `RoomsService::bind_member`:

```rust
// Proposed guard in RoomsService::bind_member
require_non_empty("room_id", &req.room_id)?;
require_non_empty("member_id", &req.member_id)?;
require_non_empty("project_id", &req.project_id)?;
require_non_empty("session_id", &req.session_id)?;
require_non_empty("agent", &req.agent)?;
require_non_empty("model", &req.model)?;
require_non_empty("delivery_mode", &req.delivery_mode)?;

// Participant Policy Guard: Colleague declarations must be Council: <non-empty member name>
let council_member = req
    .agent
    .strip_prefix("Council: ")
    .map(str::trim);

match council_member {
    Some(name) if !name.is_empty() => {}
    _ => {
        return Err(ServiceError::InvalidRequest(
            format!(
                "invalid colleague agent declaration '{}': colleague declarations are restricted to 'Council: <non-empty member name>'",
                req.agent
            )
        ));
    }
}
```

### 6.2 Atomic Failure Semantics

- If `req.agent` fails the policy check, `bind_member` returns `Err(ServiceError::InvalidRequest(...))` before acquiring an immediate write transaction or executing any database write.
- The `room_members` table is not updated.
- Incarnation counters are not incremented.

### 6.3 Preservation of Existing & Historic Records

- **No Retroactive Invalidation**: Existing room records created prior to policy activation remain completely intact. Historical deliberations stored in managed SQLite storage must not be pruned, modified, or dropped.
- **Unbound Posts Preserved**: Historical and ongoing coordinator posts made without colleague binding remain valid timeline events.
- **Migration Stability**: Migration 002 introduced columns `project_id`, `session_id`, `agent`, `model`, `delivery_mode`, and `incarnation` into `room_members`. No schema changes are required for this policy; the guard operates at the service validation layer.

---

## 7. Verification & Acceptance Criteria

When the implementation increment for `bind_member` is scheduled and authorized, it must satisfy the following verification matrix:

### 7.1 Unit & Integration Test Matrix

| Scenario | Input `agent` | Expected Outcome | State Effect |
|---|---|---|---|
| Valid Council Member | `"Council: claude-sonnet"` | `Ok(BindRoomMemberResponse)` | Member record bound in DB with incarnation updated. |
| Valid Trimmed Member | `"Council: momus"` | `Ok(BindRoomMemberResponse)` | Member record bound in DB. |
| Bare Analytical Agent | `"Athena"` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |
| Bare Council Alias | `"Momus"` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |
| Mutating Build Agent | `"hephaestus"` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |
| Mutating Task Agent | `"Sisyphus-Junior"` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |
| Empty Council Suffix | `"Council: "` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |
| Whitespace Suffix | `"Council:    "` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |
| Missing Space Prefix | `"Council:momus"` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |
| Lowercase Prefix | `"council: momus"` | `Err(ServiceError::InvalidRequest)` | Zero DB modification; member remains unbound. |

### 7.2 Incarnation Regression Invariant

Existing tests verifying incarnation fencing (such as `crates/uc-discussions/tests/auto_wake_acceptance_test.rs`) must continue to test stale incarnation rejection. When fixture agent strings in those tests are updated to conform to `"Council: <name>"`, the assertions proving monotonic incarnation progression must remain identical in behavioral semantics.

### 7.3 Real-Wire Verification

Full end-to-end verification requires:
1. Dispatch over real `subc` loopback TCP/HMAC transport using `subc-client-rs`.
2. Validation that `rooms.bind_member` returns typed error code `invalid_request` over the wire when an invalid agent declaration is submitted.
3. Database inspection confirming zero rows written or updated on rejection.

---

## 8. Summary of Responsibilities

```
+-------------------------------------------------------------------------+
|                              Host Process                               |
| (e.g. OpenCode plugin, project_room orchestrator, subagent harness)     |
|                                                                         |
|  1. Selects Read-Only Agent Profile (removes write/edit/patch/bash)     |
|  2. Enforces Real Host Tool Sandbox (blocks filesystem mutations)       |
|  3. Issues truthful binding request: agent: "Council: <name>"           |
+------------------------------------+------------------------------------+
                                     |
                          Route RPC: rooms.bind_member
                                     |
                                     v
+------------------------------------+------------------------------------+
|                         uc-discussions Daemon                           |
|                                                                         |
|  1. Validates pattern: ^Council: \S.*$                                  |
|  2. Fences monotonic incarnation                                        |
|  3. Validates active room lifecycle                                     |
|  4. Records immutable participant metadata in SQLite                    |
|                                                                         |
|  * DOES NOT sandbox filesystem     * DOES NOT verify caller binary      |
+-------------------------------------------------------------------------+
```
