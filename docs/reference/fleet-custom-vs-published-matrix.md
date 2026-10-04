# CortexKit Fleet Strategy: Published Releases vs. Custom Arcus Modules

## 1. Executive Summary & Core Architectural Principle

The maintenance overhead across our multi-agent fleet has historically been driven by maintaining local source forks of upstream CortexKit repositories (`subconscious`, `claustrum`, `insula`, `commons`, `lore`). Each upstream crate jump (such as the protocol `0.27 -> 0.28` update) created a cascade of git merges, lockfile conflicts, full multi-crate re-compilations, and local binary installations.

### The Canonical Distribution Channel Rule
To eliminate this burden, we strictly observe the separation between upstream channels and our internal distribution pipeline:
- **Official Upstream Releases**: Published to and consumed from **`npm`** (for scoped `@cortexkit/*` packages, e.g., `@cortexkit/aft-darwin-arm64`, `@cortexkit/subc-client`, `@cortexkit/log`) and **`crates.io`** (for shared Rust libraries like `subc-protocol`, `subc-transport`, `subc-client-rs`, `subc-control`).
- **Internal UltraCreative Distribution (Arcus)**: Arcus is our internal release packaging, submission, and gateway distribution pipeline for **custom modules and suite releases** (`ck-uc-discussions`, `ck-mc`, `ck-synapse`, custom `aft`), not the upstream official release registry.

### Core Architectural Principle
> **Treat CortexKit as an immutable runtime substrate.** Consume official published binaries from npm and libraries from crates.io wherever possible. Maintain custom features only in standalone modules or plugins delivered via Arcus, rather than replacing or forking entire upstream projects.

---

## 2. Component Inventory & Ownership Matrix

| Component / Project | Upstream Ownership | Custom UltraCreative Features (To Preserve) | Target Release Channel | Strategy & Action Plan |
|---|---|---|---|---|
| **`subconscious`** | Central daemon (`ck-subc`), CLI (`ck`), MCP gateway (`ck-subc-mcp`), wire headers | Embedded `crates/uc-discussions` (deliberation rooms, Athena reconciliation). | **Upstream npm / crates.io** (Platform) + **Arcus** (UCS plugin) | Decouple `uc-discussions` into an external standalone plugin. Revert `subconscious` to clean upstream mirror. |
| **`aft`** | Agent File Tools daemon (`aft`), AST tools, indexed code search | Custom tool bindings, workspace root indexing hooks, specific editor integrations. | **Arcus-delivered Module** | Keep as an Arcus-delivered module, but consume upstream `subc-*` crates via crates.io SemVer pins rather than local subconscious path dependencies. |
| **`magic-context`** | None (Own product) | Custom context transforms, session compaction, durable desk model (`ck-mc`). | **Arcus-delivered Module** | Maintain as our flagship context engine. Migrate Cargo dependencies to published crates.io pins (`0.28`, `0.27`, `0.9`, `0.25`). |
| **`synapse`** | Agent runtime mesh (`ck-synapse`), worker daemons | Custom agent execution workers and task dispatching pipelines. | **Arcus-delivered Module** | Consumes crates.io pins (`=0.27.0`, advancing to `0.28.0`). Delivers release binary via Arcus. |
| **`claustrum`** | Credential custody (`ck-claustrum`) & auth vault | None (Pure commodity). | **100% Official npm / Published** | Stop maintaining local source fork. Install pre-compiled release binary directly. |
| **`insula`** | Quota tracking daemon (`ck-insula`) | None (Pure commodity). | **100% Official npm / Published** | Stop maintaining local source fork. Install pre-compiled release binary directly. |
| **`lore`** | Lore daemon suite (`replicator`, `thalamus`, `broca`, `wake`, `mcp-gateway`) | `lore-replicator` (permanent core moat: cross-machine memory/notes replication over WireGuard mesh with natural-key provenance); `lore-thalamus` (session identity ladder & facade tokens); `lore-broca` (exact i128 nanodollar accounting); `lore-wake` (durable SQLite triggers under leases). | **Arcus-delivered Modules** (staged to `~/.local/lib/lore/bin/` for TCC policy) | Retired redundant reimplementations (`lore-daemon`, `lore-synapse`, `lore-protocol`, `lore-quota`). Phase-out `lore-mcp-gateway` once `ck-subc-mcp` supports Streamable HTTP; maintain `replicator`, `thalamus`, `broca` as Arcus keepers. |

---

## 3. The "UCS Plugin" Target Architecture

Rather than modifying `subconscious` source code to add custom fleet features, all UltraCreative custom capabilities run as independent subc child modules:

```
┌─────────────────────────────────────────────────────────────────┐
│              Official CortexKit Platform (npm)                  │
│                                                                 │
│   ck-subc (supervisor) ──┬── aft (file tools)                   │
│                          ├── ck-claustrum (credentials)         │
│                          ├── ck-insula (quota tracking)         │
│                          └── ck-bus (NATS message mesh)         │
└──────────────────────────┼──────────────────────────────────────┘
                           │ subc loopback TCP (Channel 0)
┌──────────────────────────┴──────────────────────────────────────┐
│              UltraCreative Studio Custom Modules (Arcus)        │
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  ck-uc-discussions (UCS Deliberation Service)           │   │
│   │  - Protocol: ManagementSurface                          │   │
│   │  - Features: rooms.*, council.*, peer.*                 │   │
│   │  - Storage : ~/.local/share/cortexkit/uc-discussions/   │   │
│   └─────────────────────────────────────────────────────────┘   │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  ck-mc (Magic Context Engine)                           │   │
│   │  - Protocol: ManagementSurface                          │   │
│   │  - Features: context compaction, transcript memory      │   │
│   │  - Storage : ~/.local/share/cortexkit/magic-context/    │   │
│   └─────────────────────────────────────────────────────────┘   │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │  ck-synapse (Agent Mesh Module)                         │   │
│   │  - Protocol: ManagementSurface / ToolProvider           │   │
│   │  - Features: distributed agent workers, execution mesh  │   │
│   └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Architectural Advantages
1. **Zero Git Upstream Collision**: Merging upstream updates into `subconscious` never causes merge conflicts with UCS business logic, because `uc-discussions` does not live inside upstream's repository.
2. **Independent Deployment & Upgrades**: `ck-subc` can be updated without rebuilding `ck-uc-discussions` or `ck-mc`, provided the minor wire protocol version (`0.28`) remains compatible.
3. **Decoupled Failure Domains**: A crash or migration in a custom service cannot block the core subc daemon listener or prevent other fleet modules from booting.

---

## 4. Migration & Decoupling Roadmap

### Phase 1: Decouple `uc-discussions` into a Standalone Plugin
1. Extract `crates/uc-discussions` from `subconscious` into its own repository (`Git/uc-discussions` or under `uc-studio`).
2. Replace local relative path dependencies in `crates/uc-discussions/Cargo.toml` with crates.io SemVer pins (`subc-protocol = "0.28"`, `subc-client-rs = "0.25"`, `subc-transport = "0.9"`, `subc-control = "0.27"`).
3. Package and release `ck-uc-discussions` as a dedicated Arcus suite component (`packages/arcus/ck-uc-discussions.json`).
4. Revert `subconscious/Cargo.toml` to pure upstream workspace members.

### Phase 2: Eliminate Source Forks for Pure Commodity Modules
1. **`claustrum` and `insula`**: Discontinue building these from local source forks. Configure machine setup to install pre-compiled binaries via npm or Arcus catalog.
2. **`lore`**: Decommission legacy binaries (`lore-daemon`, `lore-synapse` prototype, `lore-callosum`). Keep only actively needed micro-services and update them to published crate dependencies.

### Phase 3: Transition `aft` and `magic-context` to Published Dependency Pins
1. Verify `aft` and `magic-context` compile cleanly against crates.io versions rather than local path dependencies.
2. Package both modules cleanly for Arcus delivery, layering our custom extensions cleanly over upstream primitives.

### Phase 4: Subconscious as Fleet Wire Testing Coordinator
As the central router and connection fabric, `subconscious` maintains an end-to-end integration test harness (`crates/subc-core/tests/`) verifying that all active modules (`aft`, `magic-context`, `synapse`, `uc-discussions`) establish transport, complete HMAC authentication, pass HELLO/HELLO_ACK handshakes, and route management frames cleanly across version upgrades.
