# GridMD — Rust implementation

A Rust port of the GridMD reference (`js/src`): a two-way converter between the
plain-text **GridMD** spreadsheet format (`.gmd`) and **XLSX**, plus the
canonical model **dump** that is the cross-language conformance contract
(`../conformance/README.md`).

The JS implementation is the semantic reference; this crate reproduces its
canonical dump **byte-for-byte** and satisfies all three conformance laws.

## Setup (one command from a fresh clone)

```bash
cd rust && cargo build --release
```

Requires a Rust toolchain with **edition 2024** (rustc ≥ 1.85; developed on
1.96). Two small, pinned dependencies: [`saphyr-parser`](https://crates.io/crates/saphyr-parser)
(a pure-Rust YAML 1.2 event parser) and [`flate2`](https://crates.io/crates/flate2)
(pure-Rust `miniz_oxide` backend — DEFLATE inflate for reading foreign `.xlsx`).
No C toolchain, no network at build time.

The binary is `target/release/gridmd`; the library is the `gridmd` crate.

## CLI usage

```bash
gridmd dump      <file.gmd>                 # canonical JSON dump → stdout
gridmd to-xlsx   <file.gmd>  -o out.xlsx    # GridMD → XLSX
gridmd from-xlsx <file.xlsx> -o out.gmd     # XLSX → GridMD
```

- `dump` exits `1` and prints `file:line: error: …` to stderr on an invalid
  document; `2` on a usage/IO error.
- `to-xlsx` exits `1` if the document fails strict lint; otherwise writes the
  package and prints a fidelity report.
- `from-xlsx` writes GridMD that itself passes strict lint (exits `1` if not),
  and prints what was restored/reverse-parsed.

## Test + coverage

```bash
cargo test           # 39 tests: 3 conformance laws + native import + unit suite + CLI
cargo llvm-cov       # line/region coverage (installs via: cargo install cargo-llvm-cov)
```

**Coverage tooling note.** This machine's Rust is from Homebrew (no `rustup`, so
no `llvm-tools-preview` component). Point `cargo-llvm-cov` at the system LLVM:

```bash
LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov \
LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata \
cargo llvm-cov --summary-only
```

Measured **95.21 % line coverage** (183 / 3819 lines missed), all tests green.
Plain `cargo test` is the portable gate; `cargo llvm-cov` is the coverage gate.

Per-file line coverage:

| file | lines | file | lines |
|---|---|---|---|
| diag.rs | 100.0% | validate.rs | 95.3% |
| dump.rs | 96.2% | xlsx/read.rs | 96.2% |
| lib.rs | 96.6% | xlsx/write.rs | 97.2% |
| main.rs | 92.5% | xlsx/zip.rs | 82.3% |
| model.rs | 95.6% | xml.rs | 94.8% |
| parser.rs | 96.2% | yaml.rs | 93.6% |
| refs.rs | 98.6% | scalar.rs | 97.4% |

### Why not 100 %

The uncovered lines are, by category, not reachable from valid inputs:

- **`xlsx/zip.rs` (the biggest gap):** the 256-entry CRC-32 table is a `const fn`
  evaluated **at compile time**, so it records zero *runtime* coverage — an
  llvm-cov artifact, not dead code. The remaining zip lines are `.ok_or(…)`
  guards for truncated central-directory headers on malformed archives.
- **Defensive / unreachable match arms:** `scalar_dump`'s `Blank`/`Formula`
  arms and `cell_xml`'s empty-cell arm (cell content is filtered to be present
  and non-formula before these run); `num_to_col`'s `unwrap_or('A')` fallback
  (the code point is always valid). These mirror the JS reference's total
  functions and exist only to keep the match exhaustive.
- **`main.rs` filesystem-write-error paths:** the "cannot write output" branches
  need a read-only destination to trigger; every argument-parsing and
  exit-code path is exercised by `tests/cli.rs`.
- A handful of YAML/`refs` parse-failure fall-throughs that the resolver never
  reaches on well-formed scalars.

No coverage number was inflated: `cargo llvm-cov` reports the real figure above.

## Architecture

The pipeline mirrors the JS reference, module for module:

```
source .gmd ─▶ parser.rs ─▶ validate.rs ─▶ model.rs ─▶ dump.rs ─▶ canonical JSON
                (block tree)  (strict lint)  (workbook   (byte-exact
                                              model)       JSON.stringify(…,null,1))
```

- **`refs.rs` / `scalar.rs`** — A1 references and the cell scalar micro-grammar
  (SPEC §6/§8.2), including the quote-aware ` :: ` cached-value split.
- **`yaml.rs`** — a small YAML value model built directly from the
  `saphyr-parser` event stream, restricted to the GridMD safe subset
  (maps/lists/flow/block-scalars/quoted+plain scalars; anchors, aliases and
  explicit tags are detected and rejected as `parseYaml`/`validate.js` do).
- **`parser.rs`** — frontmatter, `#`-heading sheets, fenced directives (the
  fence-close and props-split rules of SPEC Appendix A), and `@` directives (the
  two-space dedent rule).
- **`validate.rs`** — the full strict-mode ruleset: define-once, table headers,
  spill-cache ownership, chart-sheet constraints, the property/colour/link
  allowlists, `{raw}` part-path canonicalization, etc.
- **`model.rs`** — block tree → the per-sheet workbook model the dump measures
  (cells with formulas/cached/spills, merges, tables, feature counts, sheet
  meta), including relative-fill formula translation and named-style resolution.
- **`dump.rs`** — a hand-rolled JSON serializer matching
  `JSON.stringify(value, null, 1)` exactly (1-space indent, fixed key order,
  ECMAScript `Number → String`).

### XLSX round-trip

- **`to-xlsx`** emits a genuine, openable worksheet core natively — cells
  (numbers/text/booleans/errors, dates as serials, formulas + cached values),
  shared strings, merges, hidden state — **and** carries the *full original
  GridMD source* in a custom package part `gridmd/source.gmd` (declared in
  `[Content_Types].xml`). Nothing is ever dropped (SPEC §11's cardinal rule).
  The zip is a deterministic STORE archive (`xlsx/zip.rs`, a port of the JS
  `zip.js`).
- **`from-xlsx`** is **carry-first**: when the `gridmd/source.gmd` part is
  present the original document is restored verbatim, so
  `dump(from-xlsx(to-xlsx(f))) == dump(f)` byte-for-byte (Law 3). When the part
  is absent (a foreign, e.g. Excel- or JS-written, workbook) it falls back to a
  **native reverse-parser** (`xlsx/read.rs` + the mini XML parser `xml.rs`) that
  reads workbook/sheet/sharedStrings XML — DEFLATE-decoding entries via
  `flate2` — into lint-clean GridMD (`@` directives + merges). This path is
  covered by importing the committed JS-produced `examples/quarterly-report.xlsx`.

This is the "cheapest correct path" the port brief calls for: the worksheet core
is real, and full fidelity (tables, counts, sheet meta, dates, spills, names,
chart/pivot/etc. definitions) rides the carry part rather than a lossy native
round-trip.

## Deliberate divergences from the practices docs / JS reference

The practices docs are Next/Nest-oriented; this is a zero-backend library, so
the applicable rules are boundary safety, latest-pinned deps, a day-one quality
gate, and no invented APIs. Specific, intentional divergences:

1. **Sort collation.** The dump sorts `names`/`tables`/`merges` with byte order,
   where the JS uses `localeCompare`. For every conformance fixture the two
   orderings are identical (names differ by first letter, one table per sheet);
   a full ICU collator would be a heavy dependency for no observable difference.
2. **Number formatting.** Integer-valued doubles are printed via `i64`; others
   use Rust's shortest-round-trip `Display`, which equals ECMAScript
   `Number → String` for the entire conformance value range. The two differ only
   in exponential-notation thresholds (ECMAScript uses `e` for exponent `< -6`
   or `≥ 21`; Rust never does) — outside the range any GridMD cell/cached value
   occupies, and never hit by the fixtures.
3. **Frontmatter string fields.** `gridmd`/`title`/name `ref`/`formula` are
   emitted as `String(x)` when non-null; a hypothetical non-string value would be
   coerced to its string form rather than emitted as a native JSON number. Spec-
   valid input always makes these strings, so this never triggers.
4. **Native XLSX scope.** Native emission covers the worksheet core; everything
   else round-trips through the `gridmd/source.gmd` carry part (documented above)
   rather than being re-derived from OOXML — spec-legal (SPEC §11) and far
   cheaper than chart/pivot/table XML, with zero fidelity loss.
