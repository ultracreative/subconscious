# Auto-wake deliberation: bringing project agents into a room

Status: design, r2, reviewed with shrink on 2026-09-23. The review preserved
the end-state and narrowed ownership per the program's
"design room → athena → spec campaign"
(`docs/designs/module-lifecycle-authority.md`, commit `3386a93`).

Evidence base: `docs/evidence/auto-wake-deliberation-evidence-bundle.md`. Every
claim below marked **[E-n]** cites a finding there; claims without a marker are
mine and are the ones a review should attack first.

## Review verdict

The item survives, but its ownership premise shrinks:

- `uc-discussions` owns durable rooms, attributable membership, ordered posts,
  and rejection of posts from stale member incarnations.
- The OpenCode host owns project resolution, process wake, session selection,
  agent/model selection, and interrupt/enqueue/background delivery.
- `uc-discussions` does not gain an endpoint registry, process launcher, or
  outbound host-routing loop.

The original RED test mixed both components behind a nonexistent
`rooms.convene` operation. It is split into a Rust state acceptance test and a
real OpenCode cross-project integration test. A green Rust-only
`rooms.convene` test would be green for the wrong reason because it could not
prove that a real project was woken or that a real agent turn ran.

## Why

The end-state promised by the upstream feature set: project agents are woken and
brought together to discuss an issue, the way one discusses a task with
colleagues. Three properties follow from "as if colleagues":

1. You do not have to know which desk someone is sitting at. You name the
   person; addressing is someone else's problem.
2. If they are not at their desk, they get fetched — subject to their own
   stated preference about being interrupted.
3. When they answer, the answer is attributable, and disagreement survives.

The current `uc-discussions` delivers (3) and nothing else. It is a better
mailbox: pull-based `poll_inbox`, a caller-supplied `target_session_id`, and no
notion of whether anyone is there. The four capabilities this work was meant to
obtain — auto-wake, session choice, agent/model choice, idle/interrupt/background
state — are all still delegated to the host's `prompt-async-gate`, which is the
machinery we were trying to get out from under.

This note is the corrected design. It is corrected in *shape*, not in detail: my
previous proposal (an `endpoints` table plus presence heartbeats inside
`uc-discussions`) reproduces a coupling upstream explicitly ratified against
**[E-5]**.

## What exists, read from source

Six things upstream settled that constrain any design here.

**Every live host session is itself a routable subc module [E-2].** Not a row in
a table — a module id, registered, with routes opened to it. Addressing a
colleague *is* opening a route to them.

**Route-open failure is the liveness signal, with a closed code set [E-3].**
`unknown_module`, `module_reloading`, `module_warming`, `target_unavailable`,
`module_timeout`. Retryability is decided in the protocol crate and mirrored in
both SDKs; unknown codes are terminal. There is no separate presence system to
consult, because failing to route *is* the presence answer.

**Wake policy is a preference cascade, deliberately unlike routing resolution
[E-4].** Most-specific-wins, and the contract is explicit that the two ladders
resolve differently on purpose. Wake policy is the person's stated preference
about being disturbed. It must not be unified with routing.

**Model/agent selection is a separate module with a panel op [E-4b].**
`route.select_panel` picks models for a multi-seat panel as a first-class
operation, with policy, model facts, recorded outcomes, cooldowns, quota. "Which
model answers" is a routing decision, not a room field.

**Three delivery modes are kept separate [E-4c].** `peer.*` couriers to a peer
with a claim/deliver/read lifecycle; `session.enqueue_user_message` puts words
into a specific session; `manager.prompt` drives a managed task. Upstream did not
collapse these. Our `peer.enqueue_message` conflates the first two by taking a
`target_session_id`.

**Peers key on session id, and the registry is a separate module [E-5].**
Ratified: registry state does not go in the daemon, because a bad migration would
then stop the daemon starting and take the fleet. And departure/arrival are
asymmetric — retiring a departed member MUST NOT delete undelivered messages.

Plus the house idiom for staleness: the **incarnation fence** [E-6]. Monotonic,
bumped on every (re)registration; a caller holding N cannot move a record that
advanced to N+1; an actor that went away and came back is a *different* actor.
In-memory, admission-and-status only, not a durability change.

## The ladder

Five rungs. Each is separately testable and the lower ones are useful alone.

**Rung 1 — identity is a session, not a project.** A room member is a session
id, not a project name [E-5]. `room_members.member_id` today holds
`"project:uc-studio"`, which cannot address one of three live sessions in the
same repo. Members carry `(project_id, session_id, incarnation)`.

**Rung 2 — resolution is delegated, not owned.** `uc-discussions` does not store
endpoints. It asks the registry to resolve a participant descriptor to a set of
session ids. This is the direct reversal of my previous proposal [E-5].

**Rung 3 — delivery is route-open, and its failure is the signal.** To put a turn
in front of a member: open a route to that member's session module and push
[E-2]. Do not poll. The error code returned *is* the liveness answer [E-3]:
`module_warming`/`module_reloading` → retry with backoff; `unknown_module` → the
session is not up, escalate to rung 4; `target_unavailable` → terminal for this
member, record and continue.

**Rung 4 — wake is a policy consultation, then a launch request.** On
`unknown_module`, consult the wake-policy cascade for that participant
(most-specific-wins [E-4]). If policy permits, request a launch, then retry rung
3 with bounded backoff. `uc-discussions` issues a *request*; it does not spawn.
This is what makes the A/B/C question from my previous turn moot — upstream's
shape is B (the daemon signals; something else owns launch authority), and it is
B for the registry-coupling reason [E-5], not primarily for the security reason
I gave.

**Rung 5 — the incarnation fence makes wake safe.** A woken session is a *new
incarnation* [E-6]. A turn addressed to incarnation N delivered against a member
that advanced to N+1 must be refused, not delivered. Without this, the wake path
is exactly a duplicate-injection generator: address a dead session, wake it, and
deliver a turn that was authored for the session that died. Our
`chaos_recovery_test` covers the hazard ad hoc; this replaces it with the house
mechanism, already used by `stale_route_epoch`.

Model selection sits beside this ladder, not inside it: the room carries a
routing *intent*, and seat-to-model assignment is `route.select_panel` [E-4b].

## What this costs us

Rung 1 is a breaking change to `room_members` and to every `peer.*` call that
takes `target_session_id`. Rungs 2 and 4 require a registry and a wake authority
that **are not in this checkout** — see Still open. Rung 3 inverts the transport
posture from pull to push.

This is migration 002 plus a contract break, not a patch. It should not be
commissioned before the review rules on the open questions below.

## Slicing

    A. Rung 5 alone — incarnation on the member record, fence at the
       delivery seam, refusal arm. No registry, no wake, no transport
       change. Useful immediately: it is the correctness precondition
       for any wake at all, and it is testable against HEAD today.
    B. Rung 1 — session-keyed identity, migration 002, contract break on
       peer.enqueue_message. Depends on A for the incarnation column.
    C. Rung 3 — push delivery over route.open, failure-code ladder,
       retry/backoff classification. Depends on B for addressing.
    D. Rungs 2+4 — registry resolution and wake consultation. BLOCKED:
       no registry module and no wake op signatures in this checkout.

A ships alone and should. D cannot be specified from what we have.

## Mutation controls the slices must carry

Per the house standard [E-6]: name the arm a weaker implementation passes
vacuously, then name the mutation that must fail that arm *by name and nothing
else*.

- **A:** with the incarnation comparison dropped at the delivery seam, a turn
  authored against incarnation N delivered to a member that re-registered at N+1
  must land. Must red on *the turn being accepted*, not on any error-code
  assertion. Every ordinary single-incarnation test stays green — that is the
  point of the arm.
- **B:** with session-keyed identity reverted to project-keyed, a room with two
  live sessions of the *same project* must deliver both turns to one session.
  Must red on the second session receiving nothing. A single-session-per-project
  fixture passes either way, so the fixture must carry two.
- **C:** with the retryable-code classification replaced by "retry on any error",
  a `target_unavailable` member must be retried instead of recorded terminal.
  Must red on retry count, by code name.
- **D:** unspecifiable until the wake op signatures are known.

## Still open

These are carried, not resolved. Three of them are load-bearing enough that a
review should rule before any slice is commissioned.

1. **Room fan-out semantics are unverified.** `rooms.*` exists in the observed
   capture, but nothing found states whether a room fans out to N session
   deliveries or is a server-side broadcast the sessions read. This is the
   precise question for colleague-style deliberation and it is gap 5 in the
   evidence bundle. The ladder above assumes fan-out. If it is broadcast, rung 3
   changes shape.
2. **No wake op signatures anywhere in the fleet.** The policy cascade is
   specified [E-4]; the wire shape of the wake call is not. Two named plan files —
   `.cortexkit/alfonso/plans/unified-waker-v1.md` and
   `prefrontal/.cortexkit/alfonso/plans/scheduled-wake-v1.md` — would answer this
   directly. Searched: all 27 `.cortexkit/` directories across the fleet checkout.
   Three carry `alfonso/` (`magic-context`, `aft`, `lore-wt/e2e-magic-context`);
   none carries a `plans/` subdirectory, and neither named file exists. This is a
   verified absence, not an unsearched gap.
3. **No registry module exists locally.** Rung 2 delegates to something that is
   not here. Whether we stub it, vendor a minimal one as a *separate* module
   (never inside `uc-discussions` [E-5]), or block until upstream ships, is a
   decision this note does not make.
4. **Does the fence belong on the member record or the turn?** I put incarnation
   on the member. Upstream's fence is on a registration record. These may not be
   the same object, and the difference decides whether a mid-room re-registration
   invalidates queued turns or only in-flight ones.
5. **Whether rung 5 subsumes our `leases` table.** Upstream has
   `peer.claim_undelivered`/`reset_claim` at op level [E-4c]. If the incarnation
   fence covers the same hazard, this change is net-*negative* in surface area —
   a deletion rather than an addition. Worth checking before adding anything.

## What I expect a review to attack

Most likely: that rung 5 is already fenced somewhere I did not read, making
slice A green for the wrong reason — precisely the failure mode Athena caught in
the lifecycle-authority note [E-6]. The acceptance test for A must therefore be
written against the current tree and observed RED before slice A is commissioned.
