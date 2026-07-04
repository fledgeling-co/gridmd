#!/usr/bin/env bash
# Cross-language conformance runner (conformance/README.md — the three laws).
# Usage: scripts/conformance.sh <name>=<cli-path> [<name>=<cli-path> ...]
set -uo pipefail
cd "$(dirname "$0")/.."

FIXTURES=(conformance/fixtures/01-cells conformance/fixtures/02-structure conformance/fixtures/03-features examples/quarterly-report)
fail=0

for pair in "$@"; do
  name="${pair%%=*}"; impl="${pair#*=}"
  if [ ! -x "$impl" ] && ! command -v "${impl%% *}" >/dev/null; then
    echo "$name: MISSING ($impl)"; fail=1; continue
  fi
  l1=0; l2=0; l3=0
  for f in "${FIXTURES[@]}"; do
    b=$(basename "$f")
    $impl dump "$f.gmd" 2>/dev/null | cmp -s - "conformance/expected/$b.json" && l1=$((l1+1))
    tmp=$(mktemp -d)
    if $impl to-xlsx "$f.gmd" -o "$tmp/$b.xlsx" >/dev/null 2>&1 \
      && $impl from-xlsx "$tmp/$b.xlsx" -o "$tmp/$b.gmd" >/dev/null 2>&1 \
      && $impl dump "$tmp/$b.gmd" 2>/dev/null | cmp -s - "conformance/expected/$b.json"; then
      l3=$((l3+1))
    fi
    rm -rf "$tmp"
  done
  for f in conformance/invalid/*.gmd; do
    $impl dump "$f" >/dev/null 2>&1 || l2=$((l2+1))
  done
  echo "$name: law1 dump $l1/4 · law2 reject $l2/3 · law3 roundtrip $l3/4"
  [ "$l1" = 4 ] && [ "$l2" = 3 ] && [ "$l3" = 4 ] || fail=1
done
exit $fail
