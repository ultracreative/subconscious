# Arcus Packaging, Distribution & Submission Guide

This document defines the canonical Arcus v0.4.0 packaging, dist directory hierarchy, and gateway submission standard for the `subconscious` repository.

---

## 1. Overview & Core Invariants

`subconscious` distributes its fleet binaries and daemons via the **Arcus Gateway**.

### Invariants & Boundaries

1. **Self-Contained Submission Bundles**: Releasing software via Arcus emits an immutable submission bundle to `dist/<version>/<sequence>/<component>/`.
2. **Strict Submodule Prohibition**: Consuming projects must **never** vendor the `arcus` repository as a git submodule and must **never** attempt direct git commits or pushes to an Arcus checkout. All publisher tooling is managed through `packages/arcus/bootstrap.sh` which links against the Arcus-managed `arcus-publisher` toolchain (`arcus install arcus-publisher`).
3. **Gateway Ingestion & Hydration**: Submission to the gateway is performed over authenticated HTTPS using `arcus publish submit <bundle_dir>`.
4. **Anti-Rollback Sequence Enforcement**: Monotonic sequence numbers strictly increase per package release. Coordinated multi-component suites share a single unified sequence to guarantee release-set atomicity.
5. **Distinct Digest Triples**: For every target platform payload, `arcus pack` computes and validates distinct SHA-256 hashes across the payload archive (`.tar.zst`), content source (`-content.zip`), and Wharf tree signature (`.pwr`).

---

## 2. Repository Layout & Dist Organization Standard

Following the updated standard established across Arcus and `magic-context`, all build and packaging artifacts are organized in a clean, sequence-first hierarchy under `dist/`:

```
dist/
└── <sequence>/
    ├── ck-subc/
    │   └── <version>/
    │       ├── release.json               # Signed schema-3 release envelope
    │       ├── release.index-policy.json  # Channel routing sidecar (e.g. {"channel": "stable"})
    │       ├── assets.sha256              # Sorted SHA-256 digest ledger
    │       ├── toolchain.json             # Toolchain version and provenance
    │       ├── submission.json            # Intake descriptor (conforms to submission.schema.json)
    │       ├── pack-report.json           # Machine-readable packaging record
    │       ├── ck-subc-<version>-<target>.tar.zst
    │       ├── ck-subc-<version>-<target>-content.zip
    │       └── ck-subc-<version>-<target>.pwr
    ├── ck/
    │   └── <version>/
    │       └── ...
    ├── ck-subc-mcp/
    │   └── <version>/
    │       └── ...
    └── ck-uc-discussions/
        └── <version>/
            └── ...
```

### Why Sequence-First Organization is Critical:
- **Sequence is the true immutable timeline**: Filesystem sorting by `<sequence>` directly reflects the release timeline and catalog promotion order, whereas sorting by SemVer breaks when components have different version cadences (e.g., `0.20.35` vs `0.1.10`).
- **Whole-submission atomic staging**: A single folder (`dist/<sequence>/`) contains the complete immutable set of packages and descriptors that ship together in that suite release.
- **No ambiguity**: When Arcus intake tools ingest or audit submission bundles, there is zero confusion about which version belongs to which sequence.

### Distributed Artifact Inventory

| Component ID | Software Type | Action ID | Action Executable | Action Type | Description |
|---|---|---|---|---|---|
| `ck-subc` | `service` | `start` | `ck-subc` | `executable` | Core subc routing and supervision daemon |
| `ck` | `cli` | `open` | `ck` | `executable` | Unified operator CLI |
| `ck-subc-mcp` | `cli` | `open` | `ck-subc-mcp` | `executable` | MCP stdio gateway bridging tools to subc mesh |
| `ck-uc-discussions` | `service` | `start` | `ck-uc-discussions` | `executable` | Multi-project discussions and deliberation daemon |

---

## 3. Sequence Allocation Standard

For coordinated multi-artifact releases across `subconscious`, the release orchestrator assigns a **single shared sequence** for the entire run:

$$\text{suite\_seq} = \max_{c \in \text{suite}}(\text{catalog\_seq}(c)) + 1$$

- **Strict Monotonicity**: Sequence must ALWAYS increment up and **never reset to 1**.
- **Anti-Rollback Guarantee**: Arcus client anti-rollback rules enforce $\text{requested.sequence} > \text{installed.sequence}$.
- **Compatibility Lock**: Immediate proof that all binaries in `dist/<sequence>/` came from the exact same unified suite build.
- **Eliminates Drift**: Addons and daemons do not develop mismatched per-component sequence skew.

---

## 4. Agent Instructions & Lifecycle Commands

### Quick Reference

| Intent | Command | Driver Script |
|---|---|---|
| **Bootstrap Toolchain** | `bun run bootstrap:arcus` | `packages/arcus/bootstrap.sh` |
| **Package All Components** | `bun run pack:arcus` | `scripts/pack-all-arcus.sh` |
| **Package Single Component** | `sh scripts/pack-<component>-arcus.sh` | Individual script |
| **Validate Release Envelopes** | `bun run validate:arcus` | `scripts/validate-arcus.sh` |
| **Verify Release Set Completeness** | `node scripts/lib/verify-release-set.mjs --root <dir>` | Quality gate |
| **Publish Submission Bundles** | `bun run publish:arcus` | `scripts/publish-all-arcus.sh` |
| **Direct Gateway Submit** | `arcus publish submit <bundle_dir> [--wait]` | Direct CLI |
| **Inspect Submission Status** | `arcus publish status <submission-id>` | Direct CLI |

### Detailed Workflow

#### Step 1: Bootstrap the Publisher Toolchain
```bash
bun run bootstrap:arcus
# or: sh packages/arcus/bootstrap.sh
```
- Installs `arcus-publisher` toolchain via `arcus install arcus-publisher`.
- Symlinks `packages/arcus/toolchain`.
- Symlinks `.opencode/skills/arcus-publisher`.
- Symlinks lifecycle scripts (`arcus-pipeline.sh`, `pack-arcus.sh`, `publish-arcus.sh`, `validate-arcus.sh`, `sign-arcus.sh`, `arcus-toolchain.json`, `submission.schema.json`) into `scripts/`.
- Validates the toolchain headers (`publisher toolchain accepted: 0.4.0`).

#### Step 2: Build and Package All Components
```bash
bun run pack:arcus
# or with explicit version/sequence:
sh scripts/pack-all-arcus.sh --version 0.20.8 --sequence 1
```
- Compiles release binaries with Cargo (`cargo build --release`).
- Stages binaries and runs `arcus pack` to emit signed envelopes and distinct digest triples.
- Generates `pack-report.json` for each component.
- Runs `verify-release-set.mjs` to audit target coverage, companion artifacts, and ensure zero staging or OS litter (`.DS_Store`, `Thumbs.db`).

#### Step 3: Validate Release Assets
```bash
sh scripts/validate-arcus.sh dist/0.20.8/1/ck-subc/releases/ck-subc-0.20.8-1.json
```
- Fail-closed verification of Ed25519 signatures, payload targets, and hash manifests.

#### Step 4: Submission to Gateway & Intake Transition

There are two submission paths depending on gateway feature deployment:

1. **Active Direct Ingestion (Today)**:
   - Deliver the generated bundle path under `dist/<version>/<sequence>/<component>/` via mailbox to `arcus` (`arcus-a3e4dd68`).
   - The Arcus owner executes intake via `scripts/arcus-accept-submission.sh <bundle-dir> --commit --push`.
   - Requires `"gateway": "https://arcus-auth.rustybret.com"` declared in `packages/arcus/*.json` for dynamic sequence allocation.

2. **Automated HTTPS Submission (`arcus publish submit` / Arcus 0.4.1)**:
   - When Cloudhome deploys `POST /v1/publish` on the gateway and Arcus 0.4.1 ships:
     ```bash
     # Single command pipeline with submission integration:
     sh packages/arcus/toolchain/scripts/arcus-pipeline.sh all --submit

     # Or via direct CLI:
     arcus publish submit dist/0.20.8/1/ck-subc/ --wait
     ```
   - Running `bun run bootstrap:arcus` will automatically update `packages/arcus/toolchain` to enable `--submit` natively once released.

#### Step 5: Query Submission Status
```bash
arcus publish status <submission-id>
```
- Displays verification diagnostics, hydration state, and catalog promotion progress once submitted.

---

## 5. Pruned & Obsolete Practices

To keep the repository clean and conform to Arcus v0.4.0 standards, the following practices and scripts are permanently retired:

1. **`scripts/migrate-arcus.sh`**: Deleted / excluded. Legacy v1/v2 schema migration is obsolete.
2. **`scripts/setup-arcus.sh`**: Replaced by `packages/arcus/bootstrap.sh`.
3. **Direct Git Writes**: No script or agent may execute `git commit` or `git push` against the `arcus` repository.
4. **Direct Catalog Signing**: Consuming projects do not run `arcus manifest sign-index`. Index updates are the sole domain of the Arcus catalog owner upon accepting submission bundles.
5. **V1/V2 Legacy Flags**: Flags like `--games-dir`, `--v1`, or legacy strategy blocks in update declarations are strictly rejected.
