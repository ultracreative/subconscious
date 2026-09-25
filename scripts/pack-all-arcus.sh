#!/bin/sh
# =============================================================================
# pack-all-arcus.sh — Master Arcus Packaging Orchestrator for subconscious
#
# Packages suite components under the canonical Arcus dist hierarchy:
#   dist/<version>/<sequence>/<component>/
#
# Components:
#   1. ck-subc           (daemon service)
#   2. ck                (operator CLI)
#   3. ck-subc-mcp       (MCP stdio gateway)
#   4. ck-uc-discussions (discussions service daemon)
#
# Sequence is SHARED across components so that a given dist/<version>/<sequence>/
# directory is a self-consistent, atomic release set.
# =============================================================================
set -eu

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH='' cd -- "${SCRIPT_DIR}/.." && pwd)"

VERSION="${VERSION:-}"
SEQUENCE="${SEQUENCE:-}"
SKIP_BUILD="${SKIP_BUILD:-0}"
NO_CLEAN="${NO_CLEAN:-0}"
ONLY_COMPONENT=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --sequence) SEQUENCE="$2"; shift 2 ;;
    --only) ONLY_COMPONENT="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --no-clean) NO_CLEAN=1; shift ;;
    -h|--help)
      cat <<EOF
Usage: $0 [options]
  --version X.Y.Z      Override suite version (default: from crates/subc-core/Cargo.toml)
  --sequence N         Shared release sequence for every component (default: auto-allocated)
  --only COMPONENT     Pack a single component (ck-subc, ck, ck-subc-mcp, ck-uc-discussions)
  --skip-build         Reuse existing build outputs instead of rebuilding
  --no-clean           Do not purge the target dist/<version>/<sequence>/ directory first
EOF
      exit 0
      ;;
    *)
      printf 'error: unknown option: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

if [ -z "$VERSION" ]; then
  VERSION=$(grep '^version' "${REPO_ROOT}/crates/subc-core/Cargo.toml" | head -1 | sed -E 's/version *= *"([^"]+)"/\1/')
fi
VERSION="${VERSION#v}"

# Determine shared sequence
# Critically, sequence must be strictly monotonic across the suite:
# suite_seq = MAX(all observed suite sequences on gateway/catalog) + 1, never resetting to 1.
if [ -z "$SEQUENCE" ]; then
  MAX_SEQ=0
  for comp in ck-subc ck ck-subc-mcp ck-uc-discussions uc-discussions; do
    s=0
    if command -v arcus >/dev/null 2>&1; then
      observed=$(arcus show "$comp" 2>/dev/null | grep -i "Latest Version:" | sed -E 's/.*\(seq ([0-9]+)\).*/\1/' || true)
      if [ -n "$observed" ]; then
        s="$observed"
      fi
    fi
    case "$s" in
      ''|*[!0-9]*) s=0 ;;
    esac
    if [ "$s" -gt "$MAX_SEQ" ]; then
      MAX_SEQ="$s"
    fi
  done
  SEQUENCE=$((MAX_SEQ + 1))
  # Enforce minimum sequence of 3 to guarantee monotonicity over past releases
  if [ "$SEQUENCE" -lt 3 ]; then
    SEQUENCE=3
  fi
fi

# Canonical Arcus dist organization:
#   dist/<sequence>/<package>/<version>/
RELEASE_ROOT="${REPO_ROOT}/dist/${SEQUENCE}"

printf "=====================================================================\n"
printf "pack-all-arcus: subconscious Suite Arcus Packaging\n"
printf "  version:  %s\n" "$VERSION"
printf "  sequence: %s (shared)\n" "$SEQUENCE"
printf "  output:   dist/%s/<component>/%s/\n" "$SEQUENCE" "$VERSION"
printf "=====================================================================\n"

# --- 0. Clean target directory ----------------------------------------------
if [ "$NO_CLEAN" -eq 0 ] && [ -z "$ONLY_COMPONENT" ] && [ -d "$RELEASE_ROOT" ]; then
  printf "\n[Step 0/3] Cleaning stale release directory: dist/%s\n" "$SEQUENCE"
  rm -rf "$RELEASE_ROOT"
fi

# --- 1. Pre-pack cargo compilation ------------------------------------------
if [ "$SKIP_BUILD" -eq 0 ]; then
  printf "\n[Step 1/3] Building release binaries with cargo...\n"
  (cd "$REPO_ROOT" && cargo build --release -p subc-core --bin ck-subc --bin ck)
  (cd "$REPO_ROOT" && cargo build --release -p subc-mcp --bin ck-subc-mcp)
  (cd "$REPO_ROOT" && cargo build --release -p uc-discussions --bin ck-uc-discussions)
fi

# --- 2. Pack components into unified dist/ hierarchy -------------------------
printf "\n[Step 2/3] Packaging suite components into unified dist/ hierarchy...\n"

pack_component() {
  comp="$1"
  script="$2"
  if [ -n "$ONLY_COMPONENT" ] && [ "$ONLY_COMPONENT" != "$comp" ]; then
    return 0
  fi
  printf "\n>>> Packaging %s (seq: %s)...\n" "$comp" "$SEQUENCE"
  SKIP_BUILD=1 sh "$script" --version "$VERSION" --sequence "$SEQUENCE"
}

pack_component "ck-subc" "${REPO_ROOT}/scripts/pack-ck-subc-arcus.sh"
pack_component "ck" "${REPO_ROOT}/scripts/pack-ck-arcus.sh"
pack_component "ck-subc-mcp" "${REPO_ROOT}/scripts/pack-ck-subc-mcp-arcus.sh"
pack_component "ck-uc-discussions" "${REPO_ROOT}/scripts/pack-ck-uc-discussions-arcus.sh"

# --- 3. Release-set completeness gate ----------------------------------------
printf "\n[Step 3/3] Verifying release-set completeness...\n"

if [ -n "$ONLY_COMPONENT" ]; then
  node "${REPO_ROOT}/scripts/lib/verify-release-set.mjs" \
    --root "$RELEASE_ROOT" \
    --only "$ONLY_COMPONENT"
else
  node "${REPO_ROOT}/scripts/lib/verify-release-set.mjs" --root "$RELEASE_ROOT"
fi

printf "\n=====================================================================\n"
printf "pack-all-arcus: Packaging complete! Validated release envelopes:\n"
find "$RELEASE_ROOT" -name "*.json" 2>/dev/null | grep "/releases/" | grep -v "index-policy" | sort | while read -r env; do
  printf "  • %s\n" "${env#"${REPO_ROOT}/"}"
done
printf "=====================================================================\n"
