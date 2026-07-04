#!/usr/bin/env bash
# Verb-style CLI facade over the JS implementation's three bins, so the
# cross-language conformance runner can drive it like the other ports.
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)/js"
RUN="node"
command -v bun >/dev/null && [ -f "$DIR/bunfig.toml" ] && RUN="bun"
verb="${1:-}"; shift || true
case "$verb" in
  dump)      exec $RUN "$DIR/bin/gridmd-dump.js" "$@" 2>/dev/null || exec $RUN "$DIR/bin/gridmd-dump.ts" "$@" ;;
  to-xlsx)   exec $RUN "$DIR/bin/gridmd2xlsx.js" "$@" 2>/dev/null || exec $RUN "$DIR/bin/gridmd2xlsx.ts" "$@" ;;
  from-xlsx) exec $RUN "$DIR/bin/xlsx2gridmd.js" "$@" 2>/dev/null || exec $RUN "$DIR/bin/xlsx2gridmd.ts" "$@" ;;
  *) echo "usage: gridmd-js.sh dump|to-xlsx|from-xlsx …" >&2; exit 2 ;;
esac
