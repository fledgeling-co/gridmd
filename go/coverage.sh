#!/usr/bin/env bash
# Run every test with cross-package coverage and print the aggregate + any
# lines below 100%. No make/just required — plain `go`.
set -euo pipefail
cd "$(dirname "$0")"

go test ./... -coverpkg=./... -coverprofile=cover.out
echo
echo "=== below 100% (justified in README) ==="
go tool cover -func=cover.out | grep -v '100.0%' | grep -v '^total:' || true
echo
go tool cover -func=cover.out | tail -1
