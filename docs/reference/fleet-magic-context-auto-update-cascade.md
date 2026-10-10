# Fleet Magic-Context Auto-Update Cascade and Cross-Compilation Reference

## 1. Executive Summary & Core Architectural Invariants

The CortexKit process fabric (`subconscious`) connects AI agent hosts to background tool daemons over loopback TCP. Among these modules, `magic-context` handles session compaction, dream operations, long-term memory, and context recall. Because `magic-context` spans both host-level IDE extensions and low-level daemon binaries, updates cannot occur as simple npm upgrades. They require a coordinated four-tier update cascade across the fleet.

```
┌────────────────────────────────────────────────────────────────────────┐
│ Tier 1: Host Plugin (@cortexkit/opencode-magic-context)               │
│ - Channel: Public npm registry                                         │
│ - Role: Prompt transformation, context injection, harness bridge       │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ triggers / pairs with
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Tier 2: Companion Daemon Module (ck-mc)                                │
│ - Channel: Internal Arcus distribution                                 │
│ - Role: Storage engine, dream worker, context.db WAL owner             │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ wire compatibility evaluation
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Tier 3: Agent File Tools Snapshot Isolation (aft)                      │
│ - Channel: Arcus module snapshot directory (dist-<version>.<build#>)   │
│ - Role: Code intelligence, AST tools, file editing substrate           │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ wire dependency alignment
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Tier 4: Fleet Wire Pins (lore, synapse, uc-discussions)                │
│ - Channel: crates.io exact SemVer pins (=0.x.y)                        │
│ - Role: Deliberation, quota accounting, local inference mesh           │
└────────────────────────────────────────────────────────────────────────┘
```

### The 4-Tier Dependency Hierarchy

1. **Tier 1: Host Plugin (`@cortexkit/opencode-magic-context`)**  
   The host extension runs directly inside the agent host environment, such as OpenCode or Claude Code. It intercepts user prompts, formats context compartments, and connects to the supervisor over loopback RPC. This package ships via the official public npm registry.
2. **Tier 2: Companion Daemon Module (`ck-mc`)**  
   The native binary `ck-mc` lives under `magic-context/crates/mc-module`. It supervises SQLite state, runs dream consolidation, handles background recall, and communicates over the 21-byte SubC wire protocol. It must match the data format expected by Tier 1.
3. **Tier 3: `aft` Snapshot Isolation**  
   Agent File Tools (`aft`) provides workspace indexing, symbol resolution, and patch application. Active agent sessions hold open file handles to `aft`. To prevent active builds from corrupting running sessions, `aft` releases land in immutable snapshot directories (`dist-<version>.<build#>`). Any wire protocol bump in Tier 2 requires evaluating whether `aft` must also be snapshotted against updated wire crates.
4. **Tier 4: Fleet Wire Pins (`lore`, `synapse`, `uc-discussions`)**  
   All surviving custom and keeper modules (`ck-uc-discussions`, `ck-synapse`, and `lore` microservices) bind to the core SubC wire crates: `subc-protocol`, `subc-transport`, `subc-client-rs`, and `subc-control`. Tier 4 guarantees that all fleet modules advance their wire pins together, avoiding protocol divergence on channel 0.

### Separation of Git Mirror vs. Automated PR Lifecycle vs. Workstation Runtime

Fleet maintenance strictly separates source synchronization, continuous integration, and local runtime execution into three distinct domains:

| Domain | Infrastructure Component | Responsibility |
|---|---|---|
| **Git Mirror** | Cloudhome `orw-sync` | Autonomous mirror service running on Cloudhome infrastructure. It polls upstream `cortexkit/magic-context` git repositories and updates internal read-only mirror branches without human intervention. |
| **Automated PR Lifecycle** | Cloudhome Renovate (`sj-b-worker-01`) | Scheduled automation engine running on worker node `sj-b-worker-01`. It detects upstream npm tag jumps and git tags, creates dependency update pull requests, and runs hermetic build checks before merging. |
| **Workstation Runtime** | Developer and Agent Hosts | Workstations act strictly as immutable runtime consumers. Workstations pull pre-built npm packages for Tier 1 and verified Arcus release bundles for Tier 2 and Tier 3. Workstations never compile release artifacts locally. |

### The Critical Fact: Missing Upstream Daemon Binaries

The upstream `cortexkit/magic-context` repository distributes pre-compiled binaries only for its end-user GUI application, packaged as macOS Tauri Dashboard DMGs. Upstream does **not** publish pre-compiled headless `ck-mc` daemon binaries to npm, GitHub releases, or crates.io.

Therefore, every upstream release tag requires compiling the native `ck-mc` binary from source before fleet distribution. Attempting to run a Tier 1 npm upgrade without building and distributing the corresponding Tier 2 `ck-mc` binary causes protocol skew, schema crashes, or broken RPC routes.

---

## 2. Cross-Compilation & Distribution Contract

### Why Local Workstation Compilation is Redundant

Historically, engineers compiled `ck-mc` directly on macOS Darwin workstations whenever a new npm package appeared. This practice violates fleet immutability for several reasons:

- **Host Resource Saturation**: Compiling heavy Rust workspaces saturated local CPU cores, interfered with running agent turns, and drained laptop batteries.
- **Toolchain Inconsistency**: Different workstations maintained varying versions of `rustc`, `clang`, and macOS SDK headers, producing non-reproducible binary hashes.
- **TCC Permission Hazards**: Local builds generated ad-hoc unsigned binaries, tripping macOS Transparency, Consent, and Control (TCC) security restrictions during daemon spawning.
- **Lack of Multi-Platform Artifacts**: A local developer on Apple Silicon could only produce `aarch64-apple-darwin` binaries, leaving Linux cluster hosts and Intel test machines unserviced.

### Multi-Target Rust Compilation via Cloudhome BuildKit

All native module builds take place inside standardized Linux container environments using Cloudhome BuildKit infrastructure on dedicated builders (`sj-b-worker-01`).

```
                              ┌────────────────────────────────────────┐
                              │  Cloudhome BuildKit Builder Container  │
                              │  (Linux x86_64 / sj-b-worker-01)       │
                              └──────────────────┬─────────────────────┘
                                                 │
                  ┌──────────────────────────────┼──────────────────────────────┐
                  │ cargo-zigbuild               │ cargo-zigbuild / osxcross    │
                  ▼                              ▼                              ▼
     ┌─────────────────────────┐    ┌─────────────────────────┐    ┌─────────────────────────┐
     │ aarch64-unknown-linux-  │    │ aarch64-apple-darwin    │    │ x86_64-apple-darwin     │
     │ gnu (ARM Linux)         │    │ (Apple Silicon macOS)   │    │ (Intel macOS)           │
     └─────────────────────────┘    └─────────────────────────┘    └─────────────────────────┘
                  │                                                             │
                  └──────────────────────────────┬──────────────────────────────┘
                                                 ▼
                                    ┌─────────────────────────┐
                                    │ x86_64-unknown-linux-   │
                                    │ gnu (x86 Linux Hosts)   │
                                    └─────────────────────────┘
```

The build environment combines two cross-compilation toolchains:
1. **`cargo-zigbuild`**: Uses Zig as the cross-compilation C/C++ compiler and linker driver. It handles GNU/Linux targets cleanly without requiring heavy dedicated sysroots.
2. **`osxcross`**: Provides macOS Darwin target SDK headers and the Darwin cctools linker pipeline, enabling Linux worker nodes to cross-compile Mach-O binaries for Apple platforms.

The build matrix generates binaries for four canonical fleet targets:
- `aarch64-apple-darwin` (Apple Silicon workstations and laptops)
- `x86_64-apple-darwin` (Legacy Intel macOS test runners)
- `x86_64-unknown-linux-gnu` (Cloudhome Linux infrastructure and cluster workers)
- `aarch64-unknown-linux-gnu` (ARM64 Linux edge nodes)

### Arcus Packaging Contract

Once compiled, native binaries enter the canonical Arcus packaging pipeline. Packaging follows the sequence-first directory hierarchy defined in the Arcus specification:

```
dist/
└── <sequence>/
    └── ck-mc/
        └── <version>/
            ├── release.json
            ├── release.index-policy.json
            ├── assets.sha256
            ├── toolchain.json
            ├── submission.json
            ├── pack-report.json
            ├── ck-mc-<version>-aarch64-apple-darwin.tar.zst
            ├── ck-mc-<version>-aarch64-apple-darwin-content.zip
            ├── ck-mc-<version>-aarch64-apple-darwin.pwr
            ├── ck-mc-<version>-x86_64-apple-darwin.tar.zst
            ├── ck-mc-<version>-x86_64-unknown-linux-gnu.tar.zst
            └── ck-mc-<version>-aarch64-unknown-linux-gnu.tar.zst
```

The packaging rules enforce strict invariants:
- **Monotonic Sequence Authority**: The release folder uses `dist/<sequence>/ck-mc/<version>/`, where `<sequence>` is an integer allocated by the Arcus gateway. Monotonic sequences protect the fleet against downgrade attacks and accidental rollback.
- **Signed Schema-v3 Envelopes**: Each release bundle includes a `release.json` file conforming to Arcus schema version 3, containing cryptographic signatures and asset manifests.
- **Distinct Digest Triples**: Every target archive (`.tar.zst`), content mirror (`-content.zip`), and Wharf signature (`.pwr`) produces distinct SHA-256 digests tracked in `assets.sha256`.
- **Catalog Promotion with `CATALOG-1`**: Before a build becomes available to workstation clients, the gateway verifies the bundle and signs the catalog entry with the authoritative key `CATALOG-1`. Workstations reject any catalog update missing this signature.

---

## 3. Workstation Drift Tripwire (`orw-probe.sh`)

Workstations must immediately detect when their local configuration lags behind published upstream releases. The tripwire script `scripts/fleet/orw-probe.sh` provides this validation.

### Tripwire Execution Contract

The probe queries the npm registry for the latest `@cortexkit/opencode-magic-context` distribution tag and compares it to the version pinned in the local OpenCode configuration (`opencode.json`).

```bash
# Query registry for published dist-tag
NPM_LATEST=$(curl -fsSL --connect-timeout 5 --max-time 10 https://registry.npmjs.org/@cortexkit/opencode-magic-context 2>/dev/null | jq -r '."dist-tags".latest // empty' || true)

# Extract local version pin from configuration
PINNED_VER=$(grep -oE "@cortexkit/opencode-magic-context@[0-9]+\.[0-9]+\.[0-9]+" "$PKG_JSON" 2>/dev/null | cut -d'@' -f3 || true)
```

### Exit Codes and Semantics

The script follows strict exit status semantics:

| Exit Code | Status Token | Meaning |
|---|---|---|
| **`0`** | `UP_TO_DATE` | The local workstation configuration matches the latest published upstream release. No action required. |
| **`1`** | `DRIFT_DETECTED` | The local pin differs from upstream latest. A newer release is available, and the cascade must run. |
| **`2`** | `REFUSED` | The probe encountered a network failure, request timeout, or empty registry response. It refuses to guess. |

### The Fail-Loud Property

A core design principle of fleet monitoring scripts is that missing telemetry must never masquerade as a passing health check. If the registry query fails or returns invalid JSON, `orw-probe.sh` emits an error message to `stderr` and exits with code `2`:

```bash
if [ -z "$NPM_LATEST" ]; then
    echo "orw-probe: REFUSED (npm registry unreachable)" >&2
    exit 2
fi
```

This prevents silent failures where an offline network adapter or DNS glitch would otherwise allow drift to go unnoticed.

### Automated Unit Test Verification

The probe contract is covered by unit tests in `scripts/fleet/tests/test_orw_probe.py`. The suite mocks the `curl` binary and configuration files to verify all three exit states:

```python
# test_orw_probe.py test coverage:
# 1. test_up_to_date_exits_0: Verifies code 0 when local pin equals registry latest.
# 2. test_drift_detected_exits_1: Verifies code 1 when versions diverge.
# 3. test_refused_on_network_failure_exits_2: Verifies code 2 on network error (curl exit code != 0).
# 4. test_refused_on_empty_payload_exits_2: Verifies code 2 on empty or invalid payload.
```

To run the regression suite:

```bash
python3 scripts/fleet/tests/test_orw_probe.py
```

---

## 4. Operational Runbook & Triage

### Step-by-Step Upgrade Execution Flow

When `orw-probe.sh` returns exit code 1 (`DRIFT_DETECTED`), operators execute the following procedure:

#### Phase 1: Tier 1 Host Package Staging
1. Check the newly published version on the npm registry:
   ```bash
   npm view @cortexkit/opencode-magic-context dist-tags
   ```
2. Inspect the upstream release notes and changelog for schema migrations or protocol breaking changes.
3. Update the version string in `~/.config/opencode/opencode.json`:
   ```json
   {
     "plugin": [
       "@cortexkit/opencode-magic-context@0.47.0"
     ]
   }
   ```

#### Phase 2: Tier 2 Native Daemon Build and Promotion
1. Verify that Cloudhome BuildKit on `sj-b-worker-01` has built the corresponding git tag:
   ```bash
   # Check Arcus sequence and package build status
   arcus manifest inspect --package-id ck-mc --gateway https://arcus-auth.rustybret.com
   ```
2. If an automated build did not trigger, trigger the cross-compilation pipeline on `sj-b-worker-01`:
   ```bash
   # Inside magic-context build workspace
   cargo zigbuild --release --target aarch64-apple-darwin
   cargo zigbuild --release --target x86_64-apple-darwin
   cargo zigbuild --release --target x86_64-unknown-linux-gnu
   cargo zigbuild --release --target aarch64-unknown-linux-gnu
   ```
3. Package the artifacts into the allocated sequence directory:
   ```bash
   bun run pack:arcus
   ```
4. Verify the package bundle:
   ```bash
   arcus validate "dist/${SEQUENCE}/ck-mc/${VERSION}"
   ```
5. Submit the bundle to the gateway:
   ```bash
   arcus publish submit "dist/${SEQUENCE}/ck-mc/${VERSION}" --wait
   ```
6. Ensure catalog promotion signed by `CATALOG-1` has completed.

#### Phase 3: Tier 3 and Tier 4 Wire Compatibility Assessment
1. Inspect `magic-context/crates/mc-module/Cargo.toml` to verify wire crate dependencies.
2. If `subc-protocol` or `subc-transport` versions advanced:
   - Recompile `aft` against the updated crates.
   - Export `aft` to a new isolated snapshot: `dist-<version>.<build#>`.
   - Update `subconscious/Cargo.toml`, `synapse/Cargo.toml`, and `lore` crate pins.
   - Run integration tests in `crates/subc-core/tests/` to confirm channel 0 route negotiations succeed.
3. If wire crate versions did not change, existing `aft` snapshots and Tier 4 modules remain valid.

#### Phase 4: Local Workstation Activation
1. Update `ck-mc` via the `ck` CLI:
   ```bash
   ck upgrade mc
   ```
2. Restart the daemon supervisor to pick up new module paths:
   ```bash
   # macOS
   launchctl kickstart -k gui/$(id -u)/cortexkit.subc

   # Linux systemd
   systemctl --user restart ck-subc
   ```
3. Run `orw-probe.sh` to confirm drift has resolved:
   ```bash
   ./scripts/fleet/orw-probe.sh
   # Expected output: STATUS: UP_TO_DATE (<version>)
   ```

### Store and Epoch Verification Gates

`magic-context` state is split between two separate database structures:

1. **`context.db`**: Owned by `magic-context`. Stores session compartments, memory records, transform decisions, and full-text search indexes. Located at `$XDG_DATA_HOME/magic-context/context.db` or the Cloudhome ZFS mount.
2. **`store.db`**: Owned by the SubC supervisor for module metadata. Located at `$XDG_DATA_HOME/cortexkit/<module_id>/store.db`.

Before promoting a new `ck-mc` release, operators must verify database integrity gates:

- **Clean SQLite WAL State**: The database directory must not contain stranded `-wal` or `-shm` files indicating torn transactions. Check with:
  ```bash
  sqlite3 "$XDG_DATA_HOME/magic-context/context.db" "PRAGMA integrity_check;"
  sqlite3 "$XDG_DATA_HOME/magic-context/context.db" "PRAGMA wal_checkpoint(TRUNCATE);"
  ```
- **Epoch Marker Alignment**: `context.db` contains schema epoch numbers. If a new `ck-mc` version introduces migrations, run the daemon once in check mode to ensure table migrations execute cleanly:
  ```bash
  ck-mc --verify-schema-only
  ```
- **Single-Dreamer Rule**: Only one active `ck-mc` process may act as the dream engine across the shared ZFS storage domain (`tank-main/magic-context`). Workstation instances must run with worker mode or read-only dream leases, reserving dream processing for `opencode-ops-nimbus`.

### Troubleshooting Guide

#### Issue 1: Broken Loopback Routes or Refused Handshakes
- **Symptom**: Agent commands fail with `route.open: REFUSED` or connection timeouts on loopback TCP.
- **Cause**: Wire protocol version mismatch between `ck-subc` and `ck-mc`. For example, `ck-subc` is running protocol `0.28` while `ck-mc` was compiled against `0.27`.
- **Remedy**:
  1. Inspect the running module manifests using `ck status`.
  2. Read daemon logs: `cat ~/.local/share/cortexkit/logs/subc.log`.
  3. Verify the protocol version declared in the module HELLO frame.
  4. Align crate versions in `Cargo.toml` and rebuild `ck-mc`.

#### Issue 2: Unversioned Git Transitions and Dirty Worktrees
- **Symptom**: Renovate PRs fail CI with lockfile mismatch errors or uncommitted changes.
- **Cause**: Upstream git mirror synchronization introduced dependencies that were not locked with `cargo check --locked` or `bun install`.
- **Remedy**:
  1. Check git status in the build repository: `git status --porcelain`.
  2. If `Cargo.lock` drifted, run `cargo update -p <crate>` to match the exact published version pin.
  3. Never commit directly to mirror branches. All edits must flow through Renovate PR branches on `sj-b-worker-01`.

#### Issue 3: Stale Workstation Process Holding Open Database Locks
- **Symptom**: Upgraded `ck-mc` crashes on boot with `database is locked (5)`.
- **Cause**: An older orphaned instance of `ck-mc` was not terminated cleanly and still holds an exclusive lock on `context.db`.
- **Remedy**:
  1. Identify processes holding `context.db`:
     ```bash
     lsof "$XDG_DATA_HOME/magic-context/context.db"
     ```
  2. Gracefully stop the previous process:
     ```bash
     ck stop mc
     ```
  3. If the process is unresponsive, terminate it:
     ```bash
     kill -TERM <PID>
     ```
  4. Verify the WAL file is cleaned, then restart the module via `ck start mc`.
