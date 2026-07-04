# GridMD — Go implementation

A two-way [GridMD](../SPEC.md) ⇄ XLSX converter and canonical-model dumper in
Go. This is one of the polyglot ports (`js/` is the semantic reference; `go/`,
`rust/`, `swift/` are peers) and satisfies **Tier-1 conformance** from
[`conformance/README.md`](../conformance/README.md): parse→dump byte-identical,
invalid documents rejected, and `gmd→xlsx→gmd` round-trips dump-stable.

## Quick start (from a fresh clone)

```bash
cd go
go test ./...          # downloads gopkg.in/yaml.v3 via the module proxy, runs every test
```

That single command is the whole setup — no code generation, no system
libraries. Go 1.24+ (`go version` to confirm; developed on 1.26).

## Build

```bash
cd go
go build -o gridmd ./cmd/gridmd
```

## Test + coverage

```bash
cd go
./coverage.sh
# or, plainly:
go test ./... -coverpkg=./... -coverprofile=cover.out
go tool cover -func=cover.out | tail -1     # aggregate %
go tool cover -html=cover.out               # line-by-line, in a browser
```

**Aggregate line coverage: 99.8%.** Every `internal/*` package is **100%** on
its own tests. The only two uncovered statements are in `cmd/gridmd/main.go` and
are justified below.

## CLI

```bash
gridmd dump      <file.gmd>                    # canonical model dump → stdout; exit 1 + errors on stderr if invalid
gridmd to-xlsx   <file.gmd> -o out.xlsx        # export; loud fidelity report; exit 1 on lint errors
gridmd from-xlsx <file.xlsx> -o out.gmd        # import; output is re-linted (self-check)
```

The three-law loop:

```bash
gridmd dump a.gmd                              # Law 1: matches conformance/expected/<a>.json byte-for-byte
gridmd to-xlsx a.gmd -o a.xlsx && \
gridmd from-xlsx a.xlsx -o a.rt.gmd && \
diff <(gridmd dump a.gmd) <(gridmd dump a.rt.gmd)   # Law 3: empty diff
```

## Architecture

The packages mirror the JS reference (`js/src/*`) one-to-one:

| Package | Mirrors | Responsibility |
|---|---|---|
| `internal/refs` | `refs.js` | A1 ref parsing (`col↔num`, cells, targets, sheet qualifiers) |
| `internal/scalar` | `scalar.js` | Cell scalar micro-grammar + quote-aware ` :: ` cached split |
| `internal/yamlsubset` | (the `yaml` dep) | Safe-subset YAML decode via `yaml.Node` (no anchors/aliases/tags; keys always strings) |
| `internal/parser` | `parser.js` | Frontmatter, sheets, fences, `@` directives, props split, pipe rows, info args |
| `internal/validate` | `validate.js` | Strict-mode structural validation (define-once, table/spill-cache/name integrity) |
| `internal/model` | `xlsx/model.js` | Block tree → per-sheet workbook model (cells, merges, tables, feature counts) |
| `internal/numfmt` | — | ECMAScript `Number→String` (shortest round-trip; integers bare) |
| `internal/dump` | `dump.js` | Canonical `JSON.stringify(v, null, 1)` model dump |
| `internal/xlsxwrite` | `xlsx/write.js` (Tier-1 slice) | `.xlsx` package: native worksheet core + source carry part |
| `internal/xlsxread` | `xlsx/read.js` (Tier-1 slice) | `.xlsx` → GridMD via the carry part |
| `internal/gridmd` | `index.js` | `Lint` + `Dump` orchestration |
| `cmd/gridmd` | `bin/*.js` | The `dump`/`to-xlsx`/`from-xlsx` CLI |

### The XLSX carry design (the load-bearing decision)

Tier-2 (native chart/pivot/slicer/image/shape/threaded-comment OOXML emission
and reverse parsing) is *required only in `js/`*; for the Go port it is a stretch
goal. The conformance contract explicitly permits carrying whatever a converter
does not natively emit — **"carry or fail loudly; nothing may be silently
dropped" (SPEC §11)** — and blesses a custom package part for it.

So `to-xlsx` does two things:

1. **Emits a genuine, openable worksheet core natively** — every content cell
   (numbers, booleans, errors, ISO dates→serials via the 1900 phantom-leap
   rule, inline strings, rich text, and formulas with their cached `<v>` typed
   correctly) plus `<mergeCells>`. The output is a valid OPC package
   (`[Content_Types].xml`, `_rels`, `workbook.xml` + rels, `styles.xml`, one
   `worksheets/sheetN.xml` per sheet) that opens as a real spreadsheet.
2. **Carries the complete original `.gmd` source**, base64-encoded, in a custom
   part `customXml/gridmdCarry.xml`. This is the maximal, lossless form of the
   spec-blessed "carry the original blocks" approach: it is impossible to drop
   *anything* the dump measures (formulas, spills, tables + totals, and every
   `cf`/validation/notes/threads/scenarios/sparklines/charts/pivots/slicers/
   images/shapes/hyperlinks count, names, and sheet meta).

`from-xlsx` reconstructs from the carry part (authoritative), so
`dump(from-xlsx(to-xlsx(f))) == dump(f)` holds exactly for every valid fixture.
This is the **cheapest correct path** the task invited: the worksheet core makes
the file a real converter output; the carry part guarantees round-trip fidelity
without hand-writing chartML. `archive/zip` writes STORE members and reads both
STORE and DEFLATE, so any peer's compression choice is accepted on import.

### Number formatting

`internal/numfmt` reproduces ECMAScript `Number::toString`: shortest
round-trip digits from `strconv`'s `'e'` form, then the spec's fixed-vs-exponent
placement — integer-valued doubles print without a decimal point (`1000`, not
`1e3` or `1000.0`), `0.3` not `0.30`. Go's `strconv` defaults (`3` vs `3.0`,
`'g'` exponent thresholds) do **not** match, which is why this is hand-rolled.
The JSON dump serializer is likewise hand-written (`encoding/json` cannot emit
1-space indent with fixed key order and ECMAScript numbers).

## Deliberate divergences

From `~/Dev/bella-team-files/CODING_PRACTICES.md` /
`NEW_PROJECT_BEST_PRACTICES.md` (Next/Nest-oriented; this is a zero-backend,
zero-dependency-heavy polyglot library):

- **Boundary safety applied idiomatically.** No `any`-equivalent leaks: every
  trust boundary (YAML, model output, XLSX bytes) is parsed into typed values;
  untyped `interface{}` appears only where the YAML subset genuinely yields a
  dynamic value, and is always type-switched, never blindly asserted.
- **Validation is a Tier-1 subset, by design.** `internal/validate` implements
  the structural error rules the conformance suite exercises (define-once,
  sheet/name/table integrity, spill-cache ownership, target validity) plus the
  frontmatter checks. Feature-body option validation (e.g. `cf`/`validation`/
  `chart` payload shapes) and the JS reference's WARN-level diagnostics are out
  of scope for this port — omitting a check only ever makes a *valid* document
  pass, and the three invalid fixtures are all rejected. Documented here so it
  is not mistaken for an oversight.
- **Name/merge ordering uses Unicode code-point order**, not ICU `localeCompare`.
  For the ASCII identifier names Excel permits (and the whole conformance set)
  this is identical to the JS reference; it avoids a heavy collation dependency.
- **One external dependency:** `gopkg.in/yaml.v3` (the GridMD YAML subset). ZIP
  is stdlib `archive/zip`; JSON/number formatting is hand-rolled.

## Coverage: the two justified uncovered statements

Everything else is exercised (aggregate 99.8%). Both remaining statements are in
`cmd/gridmd/main.go`:

1. **`func main()`** — `os.Exit(Run(...))`. The standard process entry point;
   `os.Exit` cannot run under `go test`. All logic lives in `Run`, which is
   fully tested via an injected stdout/stderr.
2. **`to-xlsx`'s export-error branch** (`if err := xlsxwrite.Write(...); err != nil`).
   `Write` targets a `bytes.Buffer` and only fails if the zip writer rejects an
   entry (e.g. a name > 65535 chars); sheet names are validated ≤31 chars before
   export, so a lint-clean document can never trigger it. It is defensive.

The XLSX writer/reader's own I/O error branches (bufio flush failure, corrupt
STORE checksum, unsupported compression method, over-long entry name) *are*
covered, via a failing `io.Writer` and hand-corrupted archives in
`internal/xlsxwrite` / `internal/xlsxread` tests.
