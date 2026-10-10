#!/usr/bin/env bash
# orw-probe.sh — installed-vs-latest drift tripwire for magic-context/ck-mc/aft
set -euo pipefail

# 1. Probe npm registry for latest @cortexkit/opencode-magic-context
NPM_LATEST=$(curl -fsSL --connect-timeout 5 --max-time 10 https://registry.npmjs.org/@cortexkit/opencode-magic-context 2>/dev/null | jq -r '."dist-tags".latest // empty' || true)

if [ -z "$NPM_LATEST" ]; then
    echo "orw-probe: REFUSED (npm registry unreachable)" >&2
    exit 2
fi

# 2. Check local opencode cache or opencode.json pinned version
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/opencode"
PKG_JSON="${OPENCODE_CONFIG_FILE:-$CONFIG_DIR/opencode.json}"
PINNED_VER=$(grep -oE "@cortexkit/opencode-magic-context@[0-9]+\.[0-9]+\.[0-9]+" "$PKG_JSON" 2>/dev/null | cut -d'@' -f3 || true)

echo "magic-context npm latest: $NPM_LATEST"
echo "magic-context local pin:  ${PINNED_VER:-latest}"

if [ -n "$PINNED_VER" ] && [ "$PINNED_VER" != "$NPM_LATEST" ]; then
    echo "STATUS: DRIFT_DETECTED (installed: $PINNED_VER, latest: $NPM_LATEST)"
    exit 1
else
    echo "STATUS: UP_TO_DATE ($NPM_LATEST)"
    exit 0
fi
