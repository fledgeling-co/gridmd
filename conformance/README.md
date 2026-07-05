# GridMD Conformance Suite

The language-agnostic contract every GridMD implementation in this repo
(`js/`, `go/`, `rust/`, `swift/`) must satisfy. CI runs this suite against all
implementations; a release is cut only when all pass.

## The three laws

1. **Parse + dump.** For every `fixtures/*.gmd` (and
   `../examples/quarterly-report.gmd`), strict-mode parsing succeeds with zero
   errors and the implementation's canonical model dump is **byte-identical**
   to `expected/<name>.json`.
2. **Reject invalid.** Every `invalid/*.gmd` fails strict-mode validation
   (non-zero exit / at least one error). Error *messages* are
   implementation-defined; rejection is not.
3. **Round-trip.** For every valid fixture: `dump(import(export(doc)))` equals
   `dump(doc)` — exporting to `.xlsx` and importing back must preserve the
   dumped model exactly. (`export` = gmd→xlsx, `import` = xlsx→gmd.)

## The canonical dump format

JSON, `JSON.stringify(value, null, 1)`-formatted (1-space indent), trailing
newline, UTF-8. The reference implementation is `js/src/dump.js`; its output
defines the format. Shape:

```jsonc
{
 "gridmd": "1.0",
 "title": "…" | null,
 "dateSystem": 1900 | 1904,
 "names": [ { "name", "ref"|null, "formula"|null, "value"|null } ],  // sorted by name
 "sheets": [ {
   "name", "kind": "worksheet"|"chart",
   "hidden": false|true|"very",
   "freeze": "B2"|null,
   "protected": bool,
   "cells": {                      // row-major order, content-bearing cells only
     "A1": { "t": "n|b|e|d|s", "v": … }              // scalar
          | { "t": "f", "f": "…", "cached": scalar|null, "array": "A1:B2"|null }
          | { "t": "rich", "v": "concatenated text" }
   },
   "merges": [ "A1:C1" ],          // sorted
   "tables": [ { "name", "anchor", "columns": [], "bodyRows", "hasTotals" } ],  // sorted by name
   "counts": { "cf", "validations", "notes", "threads", "scenarios",
               "sparklines", "charts", "pivots", "slicers", "images",
               "shapes", "hyperlinks" }
 } ]
}
```

Number formatting in dumps: shortest round-trip decimal of the IEEE-754 double
(ECMAScript `Number → String` semantics) — e.g. `0.3`, not `0.30`; `1000`, not
`1e3`.

## Per-implementation CLI contract

Each implementation ships an executable (or subcommand) with these behaviors:

| Command | Behavior |
|---|---|
| `<impl> dump <file.gmd>` | canonical dump to stdout; exit 1 + errors to stderr on invalid |
| `<impl> to-xlsx <file.gmd> -o out.xlsx` | export; loud fidelity report; exit 1 on lint errors |
| `<impl> from-xlsx <file.xlsx> -o out.gmd` | import; output must itself pass strict lint |

The JS reference: `js/bin/gridmd-dump.js`, `js/bin/gridmd2xlsx.js`,
`js/bin/xlsx2gridmd.js`.

## Scope tiers

- **Tier 1 (required for conformance):** everything the dump covers — the
  worksheet core (cells/scalars/formulas/cached values/spills, merges, tables,
  CF/validation/notes/threads/scenarios/sparklines counts, names, sheet
  meta/kind) plus xlsx round-trip of those fields, and `{raw}` carry-through
  for anything the implementation does not natively convert. **Nothing may be
  silently dropped** (SPEC §11) — carry or fail loudly.
- **Tier 2 (full fidelity, required in `js/`, stretch elsewhere):** native
  chart/pivot/slicer/image/shape/threaded-comment XLSX emission and reverse
  parsing, per `js/src/xlsx/`.

## Adding fixtures

Add the `.gmd` under `fixtures/`, regenerate the expectation with the JS
reference (`node js/bin/gridmd-dump.js conformance/fixtures/<f>.gmd >
conformance/expected/<f>.json`), and make sure every implementation still
passes. Never hand-edit `expected/*.json`.
