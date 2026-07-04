# GridMD — clone-and-go entry points. Requires: bun, go, rust (cargo), swift.
# macOS: brew install oven-sh/bun/bun go rust  (swift ships with Xcode CLT)

.PHONY: setup test test-js test-go test-rust test-swift conformance build

setup:
	cd js && bun install
	cd go && go mod download
	cd rust && cargo fetch
	swift package resolve

test: test-js test-go test-rust test-swift conformance

test-js:
	cd js && (test -f bunfig.toml && bun test || node --test 'test/*.test.js')

test-go:
	cd go && go test ./...

test-rust:
	cd rust && cargo test

test-swift:
	swift test

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
		swift=.bin/gridmd-swift
