#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(CDPATH='' cd -- "${SCRIPT_DIR}/.." && pwd)"

ARCUS_BIN="${ARCUS_BIN:-arcus}"
OUTPUT_DIR=""
CONFIG_FILE="${REPO_ROOT}/packages/arcus/ck-subc-mcp.json"

PACKAGE_ID="ck-subc-mcp"
SOFTWARE_TYPE="cli"
ACTION_EXECUTABLE="ck-subc-mcp"
ACTION_TYPE="executable"
SOURCE_ID="arcus"
CHANNEL="stable"
FORMAT="tar.zst"
STRATEGY="extract"

KEY_FILE="${HOME}/.config/arcus/signing.key"
SEQUENCE=""
VERSION=""
SKIP_BUILD=0
SKIP_VALIDATE=0
NO_CLEAN="${NO_CLEAN:-0}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --sequence) SEQUENCE="$2"; shift 2 ;;
    --output) OUTPUT_DIR="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --skip-validate) SKIP_VALIDATE=1; shift ;;
    --no-clean) NO_CLEAN=1; shift ;;
    --key-file) KEY_FILE="$2"; shift 2 ;;
    -h|--help)
      cat <<EOF
Usage: $0 [options] [version] [sequence]
  --version X.Y.Z     Override version
  --sequence N        Override release sequence
  --output DIR        Output directory (default: dist/<version>/<sequence>/ck-subc-mcp)
  --key-file PATH     Path to Ed25519 signing key
  --skip-build        Skip cargo compilation
  --skip-validate     Skip strict envelope validation
  --no-clean          Do not purge the target output directory first
EOF
      exit 0
      ;;
    *)
      if [ -z "$VERSION" ] && [ "${1#-}" = "$1" ]; then
        VERSION="$1"; shift
        if [ "$#" -gt 0 ] && [ -z "$SEQUENCE" ] && [ "${1#-}" = "$1" ]; then
          SEQUENCE="$1"; shift
        fi
      else
        printf 'error: unknown option: %s\n' "$1" >&2
        exit 1
      fi
      ;;
  esac
done

if [ -z "$VERSION" ]; then
  if [ -f "${CONFIG_FILE}" ]; then
    VERSION=$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${CONFIG_FILE}" | head -n 1)
  fi
fi

[ -n "$VERSION" ] || { printf 'error: unable to determine version\n' >&2; exit 1; }
VERSION="${VERSION#v}"
SEQUENCE="${SEQUENCE:-1}"
RELEASE_ID="${PACKAGE_ID}-${VERSION}-${SEQUENCE}"

if [ -z "$OUTPUT_DIR" ]; then
  OUTPUT_DIR="${REPO_ROOT}/dist/${SEQUENCE}/${PACKAGE_ID}/${VERSION}"
fi

printf 'pack-%s-arcus: packaging %s %s (seq: %s)\n' "$PACKAGE_ID" "$PACKAGE_ID" "$VERSION" "$SEQUENCE"

if [ "$SKIP_BUILD" -eq 0 ]; then
  printf 'pack-%s-arcus: building release binary with cargo...\n' "$PACKAGE_ID"
  (cd "$REPO_ROOT" && cargo build --release -p subc-mcp --bin "$ACTION_EXECUTABLE")
fi

TARGET_BIN=""
if [ -f "/Volumes/Topper2TB/.cargo-target/release/${ACTION_EXECUTABLE}" ]; then
  TARGET_BIN="/Volumes/Topper2TB/.cargo-target/release/${ACTION_EXECUTABLE}"
elif [ -f "${REPO_ROOT}/target/release/${ACTION_EXECUTABLE}" ]; then
  TARGET_BIN="${REPO_ROOT}/target/release/${ACTION_EXECUTABLE}"
fi

[ -n "$TARGET_BIN" ] && [ -f "$TARGET_BIN" ] || { printf 'error: binary not found for %s\n' "$ACTION_EXECUTABLE" >&2; exit 1; }
printf 'pack-%s-arcus: resolved binary at %s\n' "$PACKAGE_ID" "$TARGET_BIN"

TMP_STAGING=$(mktemp -d "${TMPDIR:-/tmp}/arcus-${PACKAGE_ID}-pack.XXXXXX")
trap 'rm -rf "$TMP_STAGING"' EXIT INT TERM

mkdir -p "${TMP_STAGING}/bin"
cp "$TARGET_BIN" "${TMP_STAGING}/${ACTION_EXECUTABLE}"
chmod +x "${TMP_STAGING}/${ACTION_EXECUTABLE}"
printf 'package: %s\nversion: %s\nsequence: %s\ntimestamp: %s\n' \
  "$PACKAGE_ID" "$VERSION" "$SEQUENCE" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  > "${TMP_STAGING}/release-sequence.txt"

ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) CANONICAL_TARGET="darwin-arm64" ;;
  x86_64) CANONICAL_TARGET="darwin-x64" ;;
  *) CANONICAL_TARGET="darwin-arm64" ;;
esac

if [ "$NO_CLEAN" -eq 0 ] && [ -d "$OUTPUT_DIR" ]; then
  rm -rf "$OUTPUT_DIR"
fi
mkdir -p "$OUTPUT_DIR"

printf 'pack-%s-arcus: executing arcus pack...\n' "$PACKAGE_ID"
"$ARCUS_BIN" pack \
  --package-id "$PACKAGE_ID" \
  --release-id "$RELEASE_ID" \
  --version "$VERSION" \
  --sequence "$SEQUENCE" \
  --source-id "$SOURCE_ID" \
  --channel "$CHANNEL" \
  --format "$FORMAT" \
  --action-executable "$ACTION_EXECUTABLE" \
  --action-id "open" \
  --action-type "$ACTION_TYPE" \
  --strategy "$STRATEGY" \
  --key-file "$KEY_FILE" \
  --output "$OUTPUT_DIR" \
  --target "$CANONICAL_TARGET" \
  -i "$TMP_STAGING"

ENVELOPE_PATH="${OUTPUT_DIR}/releases/${RELEASE_ID}.json"
[ -f "$ENVELOPE_PATH" ] || { printf 'error: expected envelope not found: %s\n' "$ENVELOPE_PATH" >&2; exit 1; }

cp "$ENVELOPE_PATH" "${OUTPUT_DIR}/release.json"
printf '{"channel":"%s"}\n' "$CHANNEL" > "${OUTPUT_DIR}/release.index-policy.json"
if [ -f "${SCRIPT_DIR}/arcus-toolchain.json" ]; then
  cp "${SCRIPT_DIR}/arcus-toolchain.json" "${OUTPUT_DIR}/toolchain.json"
fi
(cd "$OUTPUT_DIR" && shasum -a 256 *.tar.zst *.zip *.pwr 2>/dev/null | LC_ALL=C sort -k 2 > assets.sha256 || true)
pub_key=$(jq -r '.signatures[0].key_id // .signature.key_id // empty' "$ENVELOPE_PATH" 2>/dev/null || echo "unknown")
cat <<EOF > "${OUTPUT_DIR}/submission.json"
{
  "schema_version": 1,
  "package_id": "${PACKAGE_ID}",
  "release_id": "${RELEASE_ID}",
  "version": "${VERSION}",
  "sequence": ${SEQUENCE},
  "sequence_source": "explicit",
  "observed_max_sequence": $((SEQUENCE - 1)),
  "index_sha256_observed": "local",
  "artifact_tag": "v${VERSION}",
  "github_repo": "ultracreative/subconscious",
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "toolchain_version": "0.4.0",
  "publisher_key_id": "${pub_key}"
}
EOF

VALIDATOR="${REPO_ROOT}/packages/arcus/toolchain/scripts/validate-arcus.sh"
if [ "$SKIP_VALIDATE" -eq 0 ] && [ -f "$VALIDATOR" ]; then
  sh "$VALIDATOR" "$ENVELOPE_PATH"
fi

node "${REPO_ROOT}/scripts/lib/write-pack-report.mjs" \
  --output-dir "$OUTPUT_DIR" \
  --package-id "$PACKAGE_ID" \
  --version "$VERSION" \
  --sequence "$SEQUENCE" \
  --source-id "$SOURCE_ID" \
  --channel "$CHANNEL" \
  --target-id "$CANONICAL_TARGET" \
  --envelope "$ENVELOPE_PATH"

printf 'pack-%s-arcus: success! Envelope: %s\n' "$PACKAGE_ID" "$ENVELOPE_PATH"
