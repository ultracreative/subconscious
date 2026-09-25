#!/bin/sh
# =============================================================================
# publish-all-arcus.sh — Unified Arcus Publisher & Submission Driver for subconscious
#
# Locates submission bundles under dist/<version>/<sequence>/<component>/ and
# submits each bundle to the Arcus gateway via:
#   arcus publish submit <bundle_dir> [--gateway <url>] [--wait]
# =============================================================================
set -eu

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH='' cd -- "${SCRIPT_DIR}/.." && pwd)"

VERSION="${1:-}"
GATEWAY="${GATEWAY:-https://arcus-auth.rustybret.com}"
WAIT_SUBMIT="${WAIT_SUBMIT:-1}"
DRY_RUN="${DRY_RUN:-0}"

if [ -z "$VERSION" ]; then
  VERSION=$(grep '^version' "${REPO_ROOT}/crates/subc-core/Cargo.toml" | head -1 | sed -E 's/version *= *"([^"]+)"/\1/')
fi
VERSION="${VERSION#v}"

DIST_VERSION_DIR="${REPO_ROOT}/dist/${VERSION}"

if [ ! -d "$DIST_VERSION_DIR" ]; then
  printf "error: no packaged components found under %s\n" "$DIST_VERSION_DIR" >&2
  printf "hint: run \"sh scripts/pack-all-arcus.sh\" first.\n" >&2
  exit 1
fi

printf "=====================================================================\n"
printf "publish-all-arcus: Submitting subconscious components (%s) to Arcus\n" "$VERSION"
printf "Reading packages from: %s\n" "$DIST_VERSION_DIR"
printf "Gateway: %s\n" "$GATEWAY"
printf "=====================================================================\n"

# Locate latest sequence directory under dist/<version>/
LATEST_SEQ=$(ls -1 "$DIST_VERSION_DIR" | sort -n | tail -n 1)
[ -n "$LATEST_SEQ" ] || { printf "error: no sequence folders found in %s\n" "$DIST_VERSION_DIR" >&2; exit 1; }

SEQUENCE_DIR="${DIST_VERSION_DIR}/${LATEST_SEQ}"
printf "Target release sequence: %s\n" "$LATEST_SEQ"

for comp in ck-subc ck ck-subc-mcp ck-uc-discussions; do
  bundle_dir="${SEQUENCE_DIR}/${comp}"
  if [ ! -d "$bundle_dir" ]; then
    printf "warn: component directory not found: %s, skipping...\n" "$bundle_dir"
    continue
  fi

  printf "\n>>> Submitting component: %s (bundle: %s)...\n" "$comp" "$bundle_dir"

  # Ensure bundle has submission.json descriptor; generate if missing
  if [ ! -f "${bundle_dir}/submission.json" ]; then
    envelope_file=$(find "${bundle_dir}/releases" -name "*.json" 2>/dev/null | grep -v "index-policy" | head -n 1 || true)
    if [ -n "$envelope_file" ] && [ -f "$envelope_file" ]; then
      printf "  -> generating submission.json descriptor...\n"
      release_id=$(basename "$envelope_file" .json)
      pub_key=$(jq -r '.signatures[0].key_id // .signature.key_id // empty' "$envelope_file" 2>/dev/null || echo "unknown")
      cat <<EOF > "${bundle_dir}/submission.json"
{
  "schema_version": 1,
  "package_id": "${comp}",
  "release_id": "${release_id}",
  "version": "${VERSION}",
  "sequence": ${LATEST_SEQ},
  "sequence_source": "explicit",
  "observed_max_sequence": $((LATEST_SEQ - 1)),
  "index_sha256_observed": "local",
  "artifact_tag": "v${VERSION}",
  "github_repo": "ultracreative/subconscious",
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "toolchain_version": "0.4.0",
  "publisher_key_id": "${pub_key}"
}
EOF
    fi
  fi

  # Generate release.index-policy.json if missing
  if [ ! -f "${bundle_dir}/release.index-policy.json" ]; then
    printf '{"channel":"stable"}\n' > "${bundle_dir}/release.index-policy.json"
  fi

  # Generate assets.sha256 if missing
  if [ ! -f "${bundle_dir}/assets.sha256" ]; then
    (cd "$bundle_dir" && shasum -a 256 *.tar.zst *.zip *.pwr 2>/dev/null | LC_ALL=C sort -k 2 > assets.sha256 || true)
  fi

  # Copy toolchain.json if missing
  if [ ! -f "${bundle_dir}/toolchain.json" ] && [ -f "${REPO_ROOT}/packages/arcus/toolchain/scripts/arcus-toolchain.json" ]; then
    cp "${REPO_ROOT}/packages/arcus/toolchain/scripts/arcus-toolchain.json" "${bundle_dir}/toolchain.json"
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    printf "  [dry-run] would submit bundle %s to gateway %s\n" "$bundle_dir" "$GATEWAY"
  else
    SUBMIT_ARGS=""
    if [ "$WAIT_SUBMIT" -eq 1 ]; then
      SUBMIT_ARGS="--wait"
    fi
    printf "  -> executing arcus publish submit %s...\n" "$bundle_dir"
    arcus publish submit "$bundle_dir" --gateway "$GATEWAY" $SUBMIT_ARGS || {
      printf "warn: arcus publish submit exited non-zero for %s\n" "$comp" >&2
    }
  fi
done

printf "\n=====================================================================\n"
printf "publish-all-arcus: Submission process completed.\n"
printf "Query status anytime with: arcus publish status <submission-id>\n"
printf "=====================================================================\n"
