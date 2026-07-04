#!/usr/bin/env bash
# Runs the GridMD Swift test suite with coverage and prints an llvm-cov report
# for the library sources only (tests excluded). Run from anywhere.
set -euo pipefail
cd "$(dirname "$0")/.."   # repo root (Package.swift lives here)

swift test --enable-code-coverage

BIN_PATH=$(swift build --show-bin-path)
PROFDATA=$(find "$BIN_PATH/codecov" -name 'default.profdata' | head -1)
XCTEST=$(find "$BIN_PATH" -name '*.xctest' -type d | head -1)
EXE="$XCTEST/Contents/MacOS/$(basename "$XCTEST" .xctest)"

echo
xcrun llvm-cov report "$EXE" \
  -instr-profile "$PROFDATA" \
  -ignore-filename-regex='Tests|\.build'

# Per-line detail for a single file:
#   xcrun llvm-cov show "$EXE" -instr-profile "$PROFDATA" swift/Sources/GridMD/Yaml.swift
