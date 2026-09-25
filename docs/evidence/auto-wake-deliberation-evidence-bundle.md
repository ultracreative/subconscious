# Evidence bundle: auto-wake, session dispatch, and colleague-style deliberation

Gathered 2026-09-22 for the `uc-discussions` design note that follows. Every
claim below carries its provenance. Where a claim is an inference rather than a
reading, it says so.

The prompt for this gather: *"the ability for project agents to be auto-woken and
brought together to discuss an issue, as if one were discussing a task with
colleagues."* That end-state is not speculative — upstream **built it**, and a
capture of the running system is checked into this repository.

## The provenance situation

`subconscious` is our fork of `cortexkit/subconscious`. On 2026-09-21 the merge
`f0dc32e6` synced `upstream/master` into `ucs/uc-discussions`, which brought
upstream's `docs/` tree into our checkout. That tree is the evidence base. It is
*upstream's own design record*, not our reconstruction of it.

What we still do NOT have: the `prefrontal-core` **source**. It is unreleased.
So the ops surface below is read from a live capture and from specs, never from
an implementation. Contract-shape claims are therefore high-confidence;
implementation-detail claims are not available at all.

---

## Finding 1 — the end-state exists upstream, and its shape is captured

**Source:** `docs/fleet-surface.md:208-298` (`prefrontal-core — executive (247 ops)`)

`prefrontal-core` is described as *"the decision plane: agents, work, asks,
rooms, wakes, delegation"*. Four namespaces are directly load-bearing for us:

**`agent.*` (19)** — `docs/fleet-surface.md:213-217`
> `create`, `resolve`, `resolve_name`, `list`, `peer_roster`, `flip_status`,
> **`deliver`** (message delivery), `rename`, `update_tag`,
> `set_github_identity`, `github_identity`, **`set_sleep`**, **`wake`**,
> **`update_wake_policy`**, `dispose`, `merge`, `materialize`,
> `flip_residence`, `rebind_machine`. *Identity authority for the fleet (the
> holds-authority case).*

`agent.wake`, `agent.set_sleep`, and `agent.update_wake_policy` are **first-class
registry operations**. Waking is not a host concern in upstream's model — it is
an operation on the agent identity record.

**`rooms.* (22)`** — `docs/fleet-surface.md:242-245`
> `create`, **`invite`**, **`rsvp`**, **`enter`**, `join`, `leave`, `post`,
> **`signal`**, `poll_open/vote/close`, `grant_stage`/`release_stage`,
> **`agenda_advance`**, **`adjourn`**, `ack`, `read`, `read_for_user`,
> **`hint_wait`**, `list`, `list_for_user`, `board`.

Ours has 8 of these 22. The absent ones are exactly the colleague-conversation
verbs: `invite`/`rsvp`/`enter` (an invitation you can accept, distinct from being
silently enrolled), `signal` (out-of-band presence within a room), `hint_wait`
(tell a member the room expects them, without blocking), `agenda_advance`
(structured progression), `adjourn` (distinct from our terminal `close`).

**`manager.* (33)`** — `docs/fleet-surface.md:247-256`
> subagent/task orchestration: **`launch`**, **`prompt`**, `cancel`, `finalize`,
> `discard`, `ingest_event`, … `get_task_by_session`, …
> **`wake_parent_for_ask`**, `run_sweep`, **`run_channel_wake_sweep`**, provider
> registration/liveness (**`register_provider`**, **`heartbeat_provider`**,
> **`provider_alive`**, `set_provider_health`, …)

So upstream **does** have provider registration + heartbeat + liveness — the
presence machinery I proposed last turn. It lives in `manager.*`, alongside
`launch`/`prompt`. And `run_channel_wake_sweep` is a *sweep*: a periodic
reconciler that wakes channel participants, not a per-message push.

**`wake.* (11)`** — `docs/fleet-surface.md:268-269`
> scheduled wakes: policy set/delete/list, `effective`, schedule
> set/delete/list, `fires_list`, `fire_ack`, `create`, `author`.

`wake.effective` implies a **policy cascade with resolution** — consistent with
Finding 4 below.

**`athena.* (6)`** — `docs/fleet-surface.md:277-278`
> consult/campaign engine: `consult`, `campaign_raw`, `list_consults`,
> `get_consult`, **`spec_status`**, `cancel`.

**`council.* (3)`** — `docs/fleet-surface.md:286`
> `prepare_prompt`, `finalize`, `launch_members`.

Athena is a **two-layer** design upstream: `athena.*` is the consult/campaign
engine (the durable deliberation), `council.*` is the member-fanout mechanism
(`launch_members` → `prepare_prompt` → `finalize`). Our `council.*` implements
roughly the `council.*` layer and none of the `athena.*` layer. Note
`athena.spec_status` — the consult is bound to a **spec campaign**, matching the
`design room → athena → spec campaign` workflow.

---

## Finding 2 — THE ARCHITECTURAL KEY: one subc module per live host session

**Source:** `docs/fleet-surface.md:300-308`

> ## prefrontal-host:* (one per live host session; 10 ops each)
>
> Per-session OpenCode host bridges, registered dynamically (**34 live at
> capture** — count varies with sessions; each serves the same surface):
>
> - `host.ping`, `host.session_status`, `host.session_list`,
>   `host.session_transcript`, `host.session_transcript_page`,
>   `host.session_attachment_path`, `host.session_todos`, `host.session_exists`,
>   `host.permission_catalog`, **`host.execute_effect`**.

This is the finding that reframes everything.

**Upstream does not have the discussion daemon reach into sessions.** Every live
OpenCode session registers *itself* as a subc provider module named
`prefrontal-host:<pid>`, serving a 10-op surface. Delivery into a session is an
ordinary subc `route.open` + `call` against that session's own module, using
`host.execute_effect`. Idle/busy state is a **query** — `host.session_status`.

So the four capabilities I claimed `uc-discussions` failed to deliver are, in
upstream's design, not properties of the discussion daemon at all:

| Capability | Where it lives upstream | Mechanism |
|---|---|---|
| Session choice | `agent.resolve` → `prefrontal-host:<pid>` | registry resolves identity to a live module name |
| Idle/busy/interrupt | `host.session_status` on that module | query the session itself |
| Delivery | `host.execute_effect` on that module | ordinary subc call |
| Auto-wake | `agent.wake` + `wake.*` + `manager.launch` | registry op, policy cascade, sweep |

My proposed `endpoints` table was an attempt to rebuild `agent.resolve` +
`host.session_status` inside the discussion daemon. Upstream instead makes the
**session a routable subc peer**. That is strictly better: no heartbeat table to
go stale, because route-open failure *is* the liveness signal, and subc already
has the epoch fence for restart identity.

**Confidence:** high for the contract shape (this is a live capture, with a
concrete count of 34 live instances). Zero visibility into implementation.

---

## Finding 3 — route-open failure is the liveness signal, with exact codes

**Source:** `docs/subc-consumer-reconnect.md:159-188`

> **Route re-open MUST tolerate a transiently-absent target = retryable
> `NotSent`.** … after a daemon restart BOTH connections drop; the consumer
> reconnects and re-`route.open`s to `alfonso-host:<pid>`, but the plugin
> provider may not have re-HELLO'd yet. That MUST be a RETRYABLE `NotSent`-class
> condition … NEVER a hard terminal — **the call provably never reached a
> handler, so re-emit is safe.**

Note `alfonso-host:<pid>` — the pre-rename name for `prefrontal-host:<pid>`,
independently confirming Finding 2 from a second document.

Exact codes, *"verified at source, control.rs `handle_route_open` 715-788; the
consumer must key on the CODE, not message text"* (`:167-186`):

- **`unknown_module`** — target in neither registry nor supervisor snapshot.
  *"**THIS is ALF's steady-state transient case**: the plugin provider is
  self-connecting (not daemon-supervised), so after a restart, before it
  re-HELLOs, it falls through to `unknown_module`"*. **RETRYABLE.**
- **`module_reloading`** — drain in progress. **RETRYABLE.**
- **`target_unavailable`** — overloaded: transient *and* permanent (role
  mismatch = caller bug). **RETRYABLE-BOUNDED.**
- **`module_timeout`** — bind relay enqueued, module silent. NotSent-class,
  retryable.

> The retry MUST be bounded (cap + deadline) so a permanently-gone or
> misconfigured target eventually surfaces a terminal `NotSent` to the caller,
> not an infinite spin.

And the backoff shape (`:198-199`): *"capped exponential backoff … the provider
side used 100ms→2s, 6 attempts; match it."*

**This gives us the wake trigger for free.** A room post addressed to a dormant
project fails `route.open` with `unknown_module`. That is not an error to
surface — it is precisely the signal that the peer needs waking.

**Also load-bearing** (`:190-196`):
> **Cached route re-open on epoch change.** Cache `(target, identity) ->
> {channel, epoch}`; on epoch mismatch (a reconnect happened), re-open before
> sending.
> **Generation-scoped I/O.** A response/terminal read on the OLD socket is
> discarded — a call's result is only valid on the socket generation it was sent
> on.

A woken session is a *new generation*. Turns addressed to the old one must not
land. This is the incarnation fence, already present in the transport.

---

## Finding 4 — wake policy is a *preference cascade*, deliberately unlike routing

**Source:** `docs/specs/external-events-contract.md:241-276`

> ## Delivery half: scheduled wakes (folded 2026-08-15 from #scheduled-wakes)

and (`:268-276`):
> the user set gates agent wakes AND pushes; the push producer consults … the
> wakes ladder emits `resolution: "most_specific"`; the MCP-router ladder …
> deliberately different resolution rules — **wake policy is preference** (a …

The two ladders resolve differently **on purpose**. Wake policy is *preference*
— most-specific-wins. Do not unify it with routing resolution.

Cross-reference (`:9`, `:18`): the design lives in
`.cortexkit/alfonso/plans/unified-waker-v1.md`, and *"the delivery plane
(unified waker, status line) is DESIGNED AND …"* — i.e. designed upstream, not
yet shipped at the time of writing. `:244` names
`prefrontal/.cortexkit/alfonso/plans/scheduled-wake-v1.md` (ALF's repo).

**Neither plan file is in our checkout.** These are the two highest-value
documents we do not have. If any fork has `.cortexkit/alfonso/plans/`, those two
files answer the wake design directly.

---

## Finding 4b — model/agent selection is its own module, with a panel op

**Source:** `docs/fleet-surface.md:310-315`

> ## prefrontal-routing — model routing (10 ops)
> - `route.select` [m] / **`route.select_panel`** [m]: pick model(s) for a
>   task/panel.
> - `route.resolve_policy` [q] / `route.model_fact` [q] / `route.model_upsert`
>   [m]: routing policy and model facts.
> - `route.set_decision_outcome` [m] / `route.record_usage` [m]: feed outcomes
>   back into routing.
> - `route.record_cooldown` [m] / `route.cooldown_status` [q] /
>   `route.quota_status` [q]: provider cooldowns and quota state.

"Which model answers" is neither a room field nor a host default upstream. It is
a routing decision with policy, model facts, recorded outcomes, cooldowns and
quota — owned by a dedicated module. **`route.select_panel` selects models for a
multi-seat panel as a first-class operation**, which is exactly the council seat
assignment problem.

Consequence for us: putting `agent`/`model` columns on `room_members`, as I
proposed last turn, puts a routing decision in the messaging store. The room
should carry a routing *intent*; the selection belongs to the router.

Note also the untracked stub `crates/prefrontal-routing/` that a previous
session deleted — its name now reads as an attempt to backfill exactly this
module.

---

## Finding 4c — three distinct delivery modes, deliberately separated

**Source:** `docs/fleet-surface.md:258-262, 282-283, 247-248`

- **`peer.* (15)`** — cross-project courier: `upsert_peer`, `get_peer`,
  `list_peers`, `enqueue_message`, `get_message`, `list_messages`,
  `claim_undelivered`, `mark_delivered`, `mark_read`, `list_inbox`,
  `count_inbox`, `get_inbox_message`, `discard_message`,
  `mark_delivery_failed`, `reset_claim`.
- **`session.* (4)`** — `subscribe`, **`enqueue_user_message`**,
  `transcript_page`, `attachment_thumbnail`.
- **`manager.*`** — `launch`, **`prompt`**, `cancel`, …

Three separate ways to put words in front of an agent: courier a message to a
*peer* (async, claim/deliver/read lifecycle), enqueue directly into a *session*,
or `prompt` a *managed task*. Upstream did not collapse these. Our
`peer.enqueue_message` conflates the first and second by taking a
`target_session_id`.

Note also `peer.claim_undelivered` / `reset_claim` — the same single-writer
claim discipline our `leases` table implements, at op level.

---

## Finding 5 — the registry is deliberately a separate module

**Sources:** commit `664803f`; `docs/specs/ck-projects.md`

The ratified ruling, from the commit record:
> settled that the projects registry stays a separate module … the daemon owns
> no durable state today, so folding a journal-spine store into it hands it a
> failure class it cannot currently have — **a bad migration stops the daemon
> starting and takes the fleet**. Plus the deploy coupling, since registry
> semantics will churn.

And from `docs/specs/ck-projects.md:116-124`:
> **Derived peer rows key on session id, not directory.** The registry publishes
> member session ids directly. Directory is a live route that renames break, and
> identity riding …
>
> **Departure and arrival are not symmetric.** A member APPEARING mid-session is
> safe by default — worst case is a peer nobody messaged. A member LEAVING with
> undelivered … departure retires the derived row and **MUST NOT delete
> undelivered messages**: the failure …

Three rules for us:
1. Registry state does not go in a daemon that must start for the fleet to work.
2. **Peers key on session id, not directory.** Our `room_members.member_id` is a
   project *name* — wrong key by this rule.
3. Retiring a departed member must not destroy undelivered messages.

---

## Finding 6 — upstream's own workflow, and what an Athena review does

**Source:** `docs/designs/module-lifecycle-authority.md` (commit `3386a93`)

The commit message states the workflow:
> Next step is an Athena review of this note rather than a mason, per the
> program's **"design room → athena → spec campaign"**

The note contains an in-situ example of the review's output:
> **VERDICT: THE PREMISE IS HALF WRONG, AND THIS ITEM SHRINKS**
> Athena reviewed this note and said the route.open commit is ALREADY fenced, so
> the acceptance test below would pass at HEAD with no new record — **green for
> the wrong reason**. It instructed me to write that test against the current
> tree BEFORE commissioning anything.

The disproven original text is left unedited beneath the verdict *"so the next
reader sees what was claimed and what the source said."*

Also from that note, the incarnation definition:
> `incarnation` — monotonic, bumped on every (re)registration … a caller holding
> incarnation N cannot move a record that has advanced to N+1 … an actor that
> went away and came back is a **different** actor.
> **Not a per-frame actor** … This is admission and status only. **Not a
> durability change** — the record is in-memory.

And the acceptance-test idiom:
> the mutation: drop the incarnation comparison. Every ordinary test stays green;
> that one arm must fail by name.

**Corroboration** — `docs/designs/module-readiness-and-swap.md:3`:
> Status: design, r2 after Athena review (ct_…e9e0c01e76e8, 4-seat panel).

with sections `## Mutation controls the slices must carry` (`:339`), `## Settled
by review` (`:362`), `## Still open` (`:375`). This is the house format: a note
is revised to r2 *after* review, carries mutation controls per slice, and
separates settled from open.

---

## What this indicts in the current `uc-discussions`

1. **Wrong identity key.** `room_members.member_id` holds a project *name*.
   Upstream keys peers on **session id** (Finding 5). A project name cannot
   address one of three live sessions in the same repo.
2. **Wrong transport posture.** We poll (`peer.poll_inbox`). Upstream's live
   sessions are **routable subc modules** (Finding 2), so delivery is a
   `route.open` + push, and *failure to route is the liveness signal*
   (Finding 3).
3. **Registry state in the wrong place.** My previous proposal grew an
   `endpoints` table inside `uc-discussions`. Finding 5 is a ratified ruling
   against exactly that coupling.
4. **Ad-hoc staleness instead of the house idiom.** `chaos_recovery_test` covers
   the hazard, but upstream's mechanism is the **incarnation fence** (Finding 6),
   already used by `stale_route_epoch`.
5. **No mutation-named acceptance.** 31 green tests, none written to fail under a
   named mutation. By upstream's own standard they may be green for the wrong
   reason.

---

## Gaps in this bundle — what we could not find

1. **No upstream `prefrontal-core` source.** `fleet-surface.md` is an observed
   capture, not an implementation. Op *semantics* are inferred from names.
2. **No `wake.*` op signatures.** The policy cascade is specified; the wire shape
   of the wake call is not, in this checkout.
3. **No registry→wake integration test.** Neither repo carries a test exercising
   resolve → route-open-fail → wake → retry end to end.
4. **No council transcript.** `ct_…e9e0c01e76e8` is referenced but the archive is
   not in this tree, so the *form* of a verdict is known but not a full example.
5. **Room-vs-DM semantics unverified.** `rooms.*` exists in the capture, but
   nothing found states whether a room fans out to N sessions or is a
   server-side broadcast — the precise question for colleague-style
   deliberation.

Gap 5 is the one that blocks a complete design. Gaps 2 and 3 would likely close
it.

---

## Reading, in one line

Upstream did not build a better mailbox. It made **every live session a routable
subc peer**, put **identity and wake authority in a registry module**, and let
**route-open failure be the liveness signal**. Auto-wake is then a small thing:
resolve → route.open → on `unknown_module`, consult wake policy → wake → retry
with bounded backoff.

---

## Local backfill implementation evidence

The UCS backfill now proves the missing end-to-end behavior without moving
launcher authority into `uc-discussions`:

- `crates/uc-discussions/tests/auto_wake_acceptance_test.rs` proves durable
  session binding and rejects a stale pre-wake incarnation.
- `packages/omo-opencode/src/features/discussions/project-room-host.ts` owns
  endpoint recovery, `opencode serve` launch, session creation, agent/model
  selection, and interrupt/enqueue/background delivery.
- `packages/omo-opencode/src/features/discussions/project-room-orchestrator.ts`
  persists attributable membership and turns through the live subc module.
- The real CLI surface created room
  `room-ddcb6d7d-12f7-41f7-aa3f-4d8d12d65c09` through `project_room`, with
  responses from `resume-builder` and `magic-context`.
- The preceding wake run recorded the `resume-builder` endpoint transition
  from `offline` to `live` and stored the full room at
  `uc-studio/.omo/evidence/20260923-auto-wake-deliberation/live-run-offline-to-live.json`.

This closes gaps 3 and 5 for the UCS backfill only. It does not claim that the
unreleased upstream `prefrontal-core` contract has been recovered.
