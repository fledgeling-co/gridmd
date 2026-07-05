# GridMD — clone-and-go entry points. Requires: bun, go, rust (cargo), swift, python3.
# macOS: brew install oven-sh/bun/bun go rust  (swift ships with Xcode CLT; python3 ≥3.11)

.PHONY: setup test test-js test-go test-rust test-swift test-python conformance build

setup:
	cd js && bun install
	cd go && go mod download
	cd rust && cargo fetch
	swift package resolve
	cd python && python3 -m venv .venv && .venv/bin/pip install --quiet -e '.[dev]'

test: test-js test-go test-rust test-swift test-python conformance

test-js:
	cd js && (test -f bunfig.toml && bun test || node --test 'test/*.test.js')

test-go:
	cd go && go test ./...

test-rust:
	cd rust && cargo test

test-swift:
	swift test

test-python:
	cd python && .venv/bin/coverage run -m pytest -q && .venv/bin/coverage report --fail-under=100

build:
	cd go && go build -o ../.bin/gridmd-go ./cmd/gridmd
	cd rust && cargo build --release
	swift build -c release
	mkdir -p .bin && cp rust/target/release/gridmd .bin/gridmd-rust && cp .build/release/gridmd .bin/gridmd-swift

conformance: build
	scripts/conformance.sh \
		js="scripts/gridmd-js.sh" \
		go=.bin/gridmd-go \
		rust=.bin/gridmd-rust \
		swift=.bin/gridmd-swift \
		python="python/.venv/bin/python -m gridmd"
