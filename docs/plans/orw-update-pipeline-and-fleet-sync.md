# ORW Release Monitor, Fleet Update Pipeline & Arcus Promotion Plan

## 1. Executive Summary & Objective

Establish an automated, trigger-driven update lifecycle for the UltraCreative Studio / CortexKit fleet:
1. **Detection (ORW):** Use an Out-of-band Repository Watcher / outside condition checker to continuously monitor upstream releases of `cortexkit/magic-context` (npm `@cortexkit/opencode-magic-context` and GitHub releases/tags).
2. **Cascade Execution:** Once an upstream release is detected, execute the strict 4-tier dependency cascade:
   - **Tier 1:** Update host npm packages and extract published contracts.
   - **Tier 2:** Rebuild, test, codesign, and stage the companion native `ck-mc` daemon module (`magic-context/crates/mc-module`).
   - **Tier 3:** Rebuild and test `aft` against active SubC wire crates, exporting to a new isolated snapshot (`dist-<version>.<build#>`).
   - **Tier 4:** Synchronize CortexKit crate pins (`subc-protocol`, `subc-transport`, `subc-client-rs`, `subc-control`) across all surviving UCS modules (`ck-uc-discussions`, `ck-synapse`, `lore` keepers).
3. **Packaging & Verification:** Pack all updated components into an isolated Arcus v3 distribution sequence (`dist/<sequence>/<package>/<version>/`), run end-to-end fleet verification in an isolated testing namespace, and upon green verification, promote to stable and deploy to local and cluster hosts.

---

## 2. Trigger Mechanism: ORW (Outside Repository Watcher)

### 2.1 Watch Target & Condition
* **Primary Target:** `https://registry.npmjs.org/@cortexkit/opencode-magic-context` (`dist-tags.latest`)
* **Secondary Target:** `https://api.github.com/repos/cortexkit/magic-context/releases/latest`
* **Trigger Condition:**
  `latest_version > installed_version`
* **Implementation:**
  An autonomous outside checker (via `ctx_note` with `surface_condition` and an idempotent CronJob/script in Cloudhome `opencode-ops-nimbus`) that polls every 15–30 minutes without requiring an active human or agent session.

```bash
# Core detection probe logic
CURRENT=$(node -e 'console.log(require("@cortexkit/magic-context/package.json").version)' 2>/dev/null || echo "0.45.0")
LATEST=$(curl -fsSL https://registry.npmjs.org/@cortexkit/opencode-magic-context | jq -r '."dist-tags".latest')

if [ "$LATEST" != "$CURRENT" ] && [ -n "$LATEST" ] && [ "$LATEST" != "null" ]; then
    echo "NEW_RELEASE_DETECTED: $CURRENT -> $LATEST"
    # Trigger dispatch pipeline
fi
```

---

## 3. The 4-Tier Cascade Pipeline

```
┌────────────────────────────────────────────────────────┐
│ Tier 1: Upstream npm @cortexkit/opencode-magic-context │
└──────────────────────────┬─────────────────────────────┘
                           │ triggers
                           ▼
┌────────────────────────────────────────────────────────┐
│ Tier 2: Daemon Module ck-mc (magic-context/crates)     │
│   - Update Cargo.toml pins if wire changed             │
│   - cargo build --release --locked                     │
│   - Unit & single-store regression tests               │
└──────────────────────────┬─────────────────────────────┘
                           │ triggers
                           ▼
┌────────────────────────────────────────────────────────┐
│ Tier 3: Agent File Tools aft (aft repo)                │
│   - Verify crates/aft subc wire pins                   │
│   - cargo build --release -p agent-file-tools --bin aft│
│   - Export to new isolated dist snapshot (dist-*.#)    │
└──────────────────────────┬─────────────────────────────┘
                           │ triggers
                           ▼
┌────────────────────────────────────────────────────────┐
│ Tier 4: Fleet Module Crate Pins & UCS Extensions       │
│   - ck-uc-discussions (subconscious)                   │
│   - ck-synapse (synapse)                               │
│   - lore keepers (thalamus, broca, wake)               │
│   - Commodity pins: claustrum, insula                  │
└────────────────────────────────────────────────────────┘
```

---

## 4. Arcus Packaging, Isolation Testing & Promotion

### 4.1 Sequence Allocation (Anti-Rollback Authority)
- Sequence numbers never reset. The pipeline queries the Arcus gateway:
  ```bash
  SEQUENCE=$(arcus manifest allocate-sequence --gateway https://arcus-auth.rustybret.com --package-id <pkg>)
  ```
- For multi-package fleet trains, compute a shared monotonic sequence:
  `LUMPED_SEQ = max(all allocated package sequences)`

### 4.2 Bundle Packaging & Digest Triple
- Follow canonical sequence-first layout:
  `dist/<LUMPED_SEQ>/<package_id>/<version>/`
- Bundles must contain:
  1. `submission.json` (Schema 2)
  2. `release.json` (Signed Schema 3 envelope)
  3. `release.index-policy.json`
  4. `toolchain.json` (Toolchain 0.4.3 provenance)
  5. `assets.sha256`
  6. Artifact triple: `<pkg>-<ver>-<target>.tar.zst`, `-content.zip`, `.pwr`
- Enforce pairwise distinct digest triples and pre-submission validation:
  ```bash
  arcus validate "dist/${LUMPED_SEQ}/${PACKAGE_ID}/${VERSION}"
  ```

### 4.3 Staged Pre-Publishing on GitHub Releases
- Mandatory requirement: upload assets to GitHub release before gateway submission:
  - Tag standard: `<package_id>-v<version>_seq<sequence>`
  - Command:
    ```bash
    gh release create "${TAG}" --title "${PACKAGE_ID} v${VERSION} (seq ${LUMPED_SEQ})"
    gh release upload "${TAG}" "dist/${LUMPED_SEQ}/${PACKAGE_ID}/${VERSION}/"* --clobber
    ```

### 4.4 Isolated Fleet Test Harness (The Promotion Gate)
Before catalog promotion and before updating production hosts (`~/.config/cortexkit/subc.jsonc` and `~/.config/opencode/opencode-dev.json`):
1. **Spin up an isolated SubC daemon instance:**
   - Launch `ck-subc` in an ephemeral XDG runtime directory with test connection file.
   - Point modules at the newly packaged release binaries.
2. **Execute synthetic smoke tests:**
   - `ck-mc`: Session compaction, historian transform, dream window, `ctx_search` round-trip against temporary SQLite store.
   - `aft`: Trigram search, AST grep, LSP diagnostics, and file patch tools through SubC channel 0.
   - `ck-uc-discussions`: Deliberation room convening, turn posting, and closure verification.
   - `lore` keepers: Account tracking (`broca`), session tokens (`thalamus`), and lease triggers (`wake`).
3. **Gateway Submission & Promotion:**
   - If smoke tests PASS:
     ```bash
     arcus publish submit "dist/${LUMPED_SEQ}/${PACKAGE_ID}/${VERSION}" --wait
     ```
   - Monitor status via `arcus publish status <id>` until `published`.
4. **Production Convergence:**
   - Update `subc.jsonc` and `opencode-dev.json` to point at newly promoted versions/snapshots.
   - Restart background daemons (`launchctl kickstart -k gui/$(id -u)/cortexkit.subc`).
