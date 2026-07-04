# GridMD — Swift implementation

A pure-Swift port of the GridMD plain-text spreadsheet format: a strict
parser/linter, the canonical conformance **dump**, and a two-way **XLSX**
converter. It is the Swift member of the polyglot conformance suite (see
`../conformance/README.md`) and satisfies **Tier-1 conformance** (the three
laws) against the shared fixtures.

The JS reference in `../js/src` is the semantic source of truth; this port
mirrors its modules file-for-file (`scalar`, `refs`, `parser`, `validate`,
`xlsx/model`, `dump`, `xlsx/zip`).

## Setup — one command from a fresh clone

```bash
swift build            # from the repo root; no third-party dependencies
```

The library is pure Swift + Foundation + the system `Compression` framework.
There is **no package to install** and no network access required.

## Build, run, test

```bash
swift build -c release                 # build the library + `gridmd` CLI
swift run gridmd dump  <file.gmd>       # canonical model dump → stdout
swift run gridmd to-xlsx <file.gmd> -o out.xlsx
swift run gridmd from-xlsx <file.xlsx> -o out.gmd

swift test                             # unit + conformance suite (90 tests)
./swift/coverage.sh                    # tests + llvm-cov line-coverage report
```

### CLI contract (conformance/README.md)

| Command | Behaviour |
|---|---|
| `gridmd dump <f.gmd>` | canonical dump to stdout; exit 1 + errors on stderr if invalid |
| `gridmd to-xlsx <f.gmd> -o out.xlsx` | export; loud fidelity report; exit 1 on lint errors |
| `gridmd from-xlsx <f.xlsx> -o out.gmd` | import; output re-passes strict lint |

### Library API

```swift
import GridMD

let json  = try GridMD.dump(source)          // canonical dump (throws Failure.invalid)
let xlsx  = try GridMD.exportXLSX(source)     // .data + fidelity .report
let back  = try GridMD.importXLSX(xlsx.data)  // .gmd + .report
let lint  = GridMD.lint(source)               // .errors, .warnings, counts (never throws)
```

## SPM consumption

The **root** `Package.swift` makes the library consumable straight from the
repository URL:

```swift
.package(url: "https://github.com/…/grid-md.git", branch: "main")
// → .product(name: "GridMD", package: "grid-md")
```

Products: `.library(name: "GridMD")` and `.executable(name: "gridmd")`.
Platforms: macOS 13+, iOS 16+ (library only). Sources live under
`swift/Sources/GridMD` and `swift/Sources/GridMDCLI`; tests under
`swift/Tests/GridMDTests`.

## Design notes

- **ES number formatting (`NumberFormat.swift`).** The dump requires the
  shortest round-trip decimal exactly as JavaScript's `String(Number)` (`3` not
  `3.0`, `1e-7` not `1e-07`, `100000000000000000000` not `1e+20`). Swift's
  `Double.description` already yields the shortest *significant digits*
  (identical to ECMAScript's); we parse those digits + the decimal exponent and
  re-render them under the ECMA-262 §6.1.6.1.20 algorithm. Verified against a
  Node-generated golden table.

- **Hand-rolled YAML subset (`Yaml.swift`).** Rather than depend on a YAML
  library (whose type inference, timestamp handling and map ordering differ from
  the JS `yaml` lib's `.toJS()`), the GridMD safe subset is parsed directly:
  block maps/sequences, flow maps/sequences, literal/folded block scalars,
  single/double-quoted and plain scalars with core-schema typing, and `#`
  comments. This gives exact control over the semantics the dump depends on
  (e.g. the flow scalar `B9:B11` staying one string) and keeps the package
  dependency-free. The spec permits hand-rolling the subset.

- **XLSX strategy (`Xlsx.swift`) — cheapest correct round-trip.** `to-xlsx`
  emits the **worksheet core natively** (cells with the right OOXML types +
  merges) so the produced `.xlsx` is a genuine, openable spreadsheet, **and**
  carries the *entire original GridMD source* (base64) in a custom package part
  `customXml/gridmdCarry.xml`. `from-xlsx` restores losslessly from that part.
  Nothing is ever silently dropped (SPEC §11's cardinal rule). This is the
  approach the port-runner contract explicitly sanctions ("carry GridMD
  definitions in a custom package part rather than re-authoring
  chart/pivot/slicer/image/shape OOXML") and makes Law 3 (`dump(import(export))
  == dump`) hold for every measured field — cells/formulas/cached/spills,
  merges, tables, all feature counts, names and sheet meta — because the
  re-emitted document is byte-identical to the input.

- **ZIP (`Zip.swift`).** A deterministic STORE writer (port of `zip.js`) plus a
  reader that handles STORE and DEFLATE; DEFLATE entries (produced by other
  tools) are inflated with Apple's `Compression` framework, whose
  `COMPRESSION_ZLIB` is raw DEFLATE — exactly ZIP method 8.

- **No `any`/unchecked casts at boundaries.** Untrusted input (source text,
  YAML, `.xlsx` bytes) is parsed into typed values; parse failures are
  diagnostics, not crashes. There are no force-unwraps on untrusted input.

## Test coverage

90 unit + conformance tests. Coverage command:

```bash
swift test --enable-code-coverage
xcrun llvm-cov report \
  "$(swift build --show-bin-path)/GridMDPackageTests.xctest/Contents/MacOS/GridMDPackageTests" \
  -instr-profile "$(swift build --show-bin-path)/codecov/default.profdata" \
  -ignore-filename-regex='Tests|\.build'
```

**Reported line coverage: 97.67%** (llvm-cov summary, library sources only).

At the source-line level, exactly **three** lines never execute — all defensive
branches that are unreachable given the validated inputs that reach them:

| Line | Why unreachable |
|---|---|
| `Dump.swift:125` — `objName` `return ""` fallback | every dumped name/table object is built with a string `name`, so the "no name field" branch cannot be hit. |
| `Validate.swift:313` — `default: break` in the fence switch | every reserved directive kind is either an explicit case or intercepted (`sheet`/`spill-cache`) before `validateFence`; no kind reaches `default`. |
| `Zip.swift:190` — `inflateRaw` positive-after-doublings tail | the buffer-doubling loop returns as soon as the output fits; the post-loop "positive but still capacity-equal after 8 doublings" return is a belt-and-braces guard. |

The gap between 97.67% and 100% in the llvm-cov *summary* metric is
region-accounting for Swift's closure-dense code (many small `.map`/`.filter`
and nested-function regions), not additional dead source lines — the three above
are the only lines that genuinely never run.

## Deliberate divergences from the practices docs

The practices docs (`~/Dev/bella-team-files/{CODING_PRACTICES,NEW_PROJECT_BEST_PRACTICES}.md`)
are Next/Nest-oriented; this is a zero-backend, dependency-free polyglot
library. Applicable rules are followed (boundary type-safety, no invented APIs,
latest toolchain, day-one quality gate via `swift test`, self-review). Divergences:

- No web framework, database, DI container, or env/secrets surface — N/A.
- **Anchors/aliases/tags** in YAML are simply not produced by the GridMD subset;
  this port does not add explicit rejection of them (the JS reference does), a
  minor lenience that never affects the conformance fixtures (which contain
  none). Everything else in the safe subset is parsed faithfully.
- CLI arg parsing is dependency-free (manual argv) rather than
  swift-argument-parser, to keep the package free of third-party dependencies.
