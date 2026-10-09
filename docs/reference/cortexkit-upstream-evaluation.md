# Upstream CortexKit Ecosystem Investigation & Adoption Evaluation

**Date:** October 8, 2026  
**Status:** Comprehensive Architectural Evaluation  
**Subject:** Upstream CortexKit Repositories (`basal`, `entorhinal`, `fusiform`, Org Census) vs. UCS Fleet Implementations

---

## 1. Executive Summary

We conducted a deep architectural survey across the entire CortexKit upstream organization (17 public repositories), with focused investigations into **`basal`**, **`entorhinal`**, and **`fusiform`**. 

Our primary architectural objective is **decoupling and eliminating custom forks**: wherever upstream provides an authoritative, maintained crate or service, we should adopt it directly rather than maintaining local forks, custom shims, or redundant implementations.

### Strategic Adoption Matrix

| Upstream Component | Architectural Role | Current UCS Overlap | Adoption Recommendation |
|---|---|---|---|
| **`cortexkit/entorhinal`** | Authoritative Project & Agent Registry (`project-identity/v1`, `agent-identity/v1`) | `uc-studio` project-mail directory, `~/.omo/presence/<id>.json`, project listing | **ADOPT (High Priority).** Replace custom project directory & root resolution. Retain uc-studio launch policy overlay & active session fences. |
| **`cortexkit/fusiform`** | Provenance-bearing Model Capability & Exact Pricing Catalog (`catalog.get`, `catalog.history`, `plan.prices`) | `uc-studio` model-capabilities tools, OpenCode models-dev cache | **ADOPT via Adapter (Medium Priority).** Use as authoritative source for pricing, fact eras, and capability metadata. Keep request-shaping and endpoint configs host-owned. |
| **`cortexkit/basal`** | Approved, Journaled, Replay-Safe Automation Workflows (Confined QuickJS, BLAKE3 consent) | Ad-hoc agent loops, cron scripts, uncoordinated wake-up triggers | **PILOT for Schedules (Medium Priority).** Adopt for scheduled workflows. Defer event-driven routing: upstream event plane is currently unpopulated in code. |
| **`cortexkit/common-auth`** | Shared provider auth, account pools, and Claustrum custody plumbing | Custom auth shims in `uc-studio` | **ADOPT via Upstream Plugins.** Consume through official auth plugins (`openai-auth`, `anthropic-auth`) rather than direct linking. |

---

## 2. In-Depth Analysis: `cortexkit/entorhinal` (Project Registry)

### 2.1 What it is
`entorhinal` is a Rust workspace (`entorhinal-core 0.3.0`, `entorhinal-module 0.1.20`, binary: `ck-entorhinal`) providing an SQLite journal-backed registry for **projects, roots, workspaces, and agents**. It implements SubC management surface protocols `project-identity/v1` and `agent-identity/v1`.

### 2.2 Core Capabilities
1. **Durable Identity vs. Volatile Presence:** It explicitly decouples *who a project/agent is* from *where it is currently running*.
   - Projects receive stable `pj-...` identifiers, aliases, and root bindings.
   - Root records track worktrees and prevent accidental inheritance across nested `.git` boundaries.
   - It supports an optional shared identity log across machines coordinated via `engram`.
2. **Session Liveness Mirror:** Provides `projects.session_liveness` (seq-tracked volatility feed), updating `lastRouteActivityMs` without polluting the durable store with transient PID/port data.
3. **Incarnation Guarantees:** Employs process incarnation nonces (8-byte random nonce at startup) and registry generations to invalidate stale client caches.

### 2.3 Comparison with `uc-studio`
* **Current UCS:** Uses ad-hoc directory listings, project mailbox path heuristics, and `~/.omo/presence/<projectId>.json` files (30s TTL heartbeat).
* **The Seam:** `entorhinal` should become the authoritative answer to **"Which projects and roots exist?"**. However, `entorhinal` does *not* replace the live session router: uc-studio must continue to manage host process spawning, launch policy, and our monotonic session incarnation fence in `uc-discussions`.

---

## 3. In-Depth Analysis: `cortexkit/fusiform` (Model Catalog)

### 3.1 What it is
`fusiform` is a SubC daemon module (`ck-fusiform`, `fusiform-protocol 0.28.0`) that continuously ingests, normalizes, and journals LLM model capabilities, token limits, and exact pricing from `models.dev/api.json`.

### 3.2 Core Capabilities
1. **Fact Eras & Auditability:** Instead of a static JSON snapshot, Fusiform records **eras** and observations. If a model drops from an API, it creates a retirement tombstone rather than silently deleting it. Historical queries (`catalog.history` with `at_ms`) enable post-hoc cost and routing audits.
2. **Exact Money & Billing Planes:** Replaces floating-point prices with exact decimal integers + exponent (`priced`, `stated_zero`, `unpriced`, `billed_as`), distinguishing creator list prices from reseller billing planes (OpenAI, Anthropic, DeepSeek, xAI).
3. **Explicit Unknowns:** Distinguishes between `null` (unknown/unpublished) and `false` or `0` (explicitly unsupported/free).

### 3.3 Comparison with `uc-studio`
* **Current UCS:** `uc-studio` bundles `model-core` and runtime snapshots (`model-capabilities-snapshot.ts`), while OpenCode maintains its own `models-dev.ts` cache.
* **The Seam:** Fusiform should supply the **ground-truth facts and pricing** over SubC channel 0 (`catalog.get`). However, **request shaping, temperature/top-p settings, provider endpoint URLs, and SDK selection must remain host-owned**.

---

## 4. In-Depth Analysis: `cortexkit/basal` (Flow Engine)

### 4.1 What it is
`basal` is a Rust workspace (`ck-basal`, QuickJS-NG 0.16.2 confined worker `ck-basal-worker`) providing operator-approved, journaled, deterministic automation scripts.

### 4.2 Core Capabilities
1. **Sandboxed Replay Safety:** Scripts run in Seatbelt-confined QuickJS VMs on macOS without network/filesystem access, communicating with `ck-basal` over versioned IPC. The parent journals all external effects.
2. **BLAKE3 Approval Binding:** Workflows are approved by exact-hash cards over `(manifest || script)`. Any edit immediately invalidates the approval.
3. **Digest Sinks:** Flow scripts can write structured items to agent digest sinks with explicit attention budgets (`silent < piggyback < wake`).

### 4.3 Current Limitations & Fleet Fit
* **Worker Confinement is macOS-Only:** The production worker currently requires macOS Seatbelt sandboxing; it cannot run unmodified on Linux cluster nodes.
* **Missing Production Event Plane:** While the manifest schema models event triggers (`trigger.events`), the production `SubcCatalog::event()` currently returns `None` (`event_not_declared`).
* **The Seam:** Do not attempt to replace `project_message` or event bus routing with Basal today. Instead, adopt Basal for **scheduled automations (cron/interval workflows)** that query repository state and post digests.

---

## 5. Complete CortexKit Organization Census (17 Repositories)

Our survey of all 17 public repositories under `github.com/cortexkit` yields four clear functional categories:

```
┌────────────────────────────────────────────────────────────────────────┐
│ 1. Core Runtime & Infrastructure                                      │
│    - subconscious (ck-subc, ck CLI, wire protocol, ck-bus, release)   │
│    - commons (shared types, leases, logging, path normalization)      │
├────────────────────────────────────────────────────────────────────────┤
│ 2. Tool & Agent Services (SubC Daemon Modules)                         │
│    - aft (Agent File Tools: AST, symbols, search, edit)                │
│    - magic-context (Persistent context transforms, desk compaction)   │
│    - claustrum (Encrypted credential custody, token refresh)           │
│    - insula (Provider quota observation & window tracking)             │
│    - synapse (Local embeddings, reranking, certified inference)       │
├────────────────────────────────────────────────────────────────────────┤
│ 3. Decision, Identity & Catalog Services                               │
│    - entorhinal (Durable project & agent identity registry)           │
│    - fusiform (Model capability, limit, and pricing catalog)          │
│    - basal (Approved, journaled QuickJS automation flows)             │
├────────────────────────────────────────────────────────────────────────┤
│ 4. Authentication, Release & Auxiliary Adapters                        │
│    - common-auth (Shared OAuth & transport core for providers)        │
│    - openai-auth (ChatGPT/Codex OAuth & pool management)              │
│    - anthropic-auth (Anthropic OAuth & credit management)             │
│    - antigravity-auth (Google Antigravity transport - caution on ToS) │
│    - orw (Outside Repo Watcher: OpenCode release/integration tracker) │
│    - opencode-interceptor (Session request/response debug capture)    │
│    - tree-sitter-scss (MSVC-fixed SCSS grammar crate)                 │
└────────────────────────────────────────────────────────────────────────┘
```

*Note on Referenced Internal Modules:* Upstream documentation references `prefrontal` (executive/session layer), `broca` (durable LLM run engine), `astrocyte` (spend metering), and `engram` (backup service). These are private or unreleased repositories outside the 17 public repos.

---

## 6. Phased Decoupling & Adoption Roadmap

### Phase 1: Deploy `entorhinal` for Central Project Registry
1. Deploy `ck-entorhinal` under `ck-subc` supervision in `~/.config/cortexkit/subc.jsonc`.
2. Update `uc-studio`'s project discovery to query `ck-entorhinal` (`resolve`, `enumerate`) instead of scanning raw directories.
3. Keep launch policies, endpoint presence, and room session incarnation fences in `uc-studio` / `uc-discussions`.

### Phase 2: Integrate `fusiform` via Host Adapter
1. Supervise `ck-fusiform` under `ck-subc`.
2. Connect `uc-studio`'s model capability queries to `catalog.get` over SubC channel 0.
3. Transition token pricing and context limits to Fusiform's exact decimal values, eliminating custom static price tables.

### Phase 3: Pilot `basal` for Scheduled Workspace Tasks
1. Supervise `ck-basal` on local macOS workstations.
2. Author initial scheduled flows (e.g., nightly Git branch stale-pruning, repository observation).
3. Connect digest outputs to agent attention sinks.

### Phase 4: Standardize Provider Auth on `common-auth`
1. Consume upstream provider plugins (`openai-auth`, `anthropic-auth`) bundled with `common-auth`.
2. Eliminate custom token refresh loops and redundant keyring shims.
