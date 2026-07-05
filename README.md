# GridMD

**A Markdown-like, portable plain-text format for full-fidelity spreadsheets.**

GridMD (`.gmd`) is to spreadsheets what Markdown is to documents: a concise,
human-readable, line-oriented text format that an AI can read and author reliably,
a database can store and address at block level, a human can edit in any text
editor, and a converter can round-trip to and from XLSX/ODS.

````gridmd
---
gridmd: "0.1"
title: Mini example
names:
  - { name: TaxRate, ref: "Assumptions!$B$2" }
---

# Financials

@ A1 "Q3 Earnings Report" { bold: true, size: 14 }
@ B2 =SUM(B4:B10) :: 45020.5 { numfmt: "$#,##0.00" }

```{table} Sales at A4
style: medium-2
cols: { price: { numfmt: "$#,##0.00" } }
---
| id | product  | qty | price | total            |
| 1  | Widget A | 45  | 12.99 | =[@qty]*[@price] |
| 2  | Widget B | 12  | 19.50 | =[@qty]*[@price] |
```

```{chart} bar "Revenue by product" at E2:K16
series:
  - { name: Revenue, cat: Sales[product], val: Sales[total] }
```
````

## Why

Spreadsheet serialization today forces a bad choice. OOXML/ODS carry 100 % of
Excel's feature surface but are XML archives — token-hostile, undiffable,
unreadable. CSV/Markdown tables are concise and readable but lose formulas,
formatting, and everything structural. JSON models (SheetJS, Luckysheet) keep the
model but drown formulas in escape characters — the exact failure mode that makes
LLM generation unreliable.

GridMD takes the empirically strongest ideas from the prior art:

| Idea | Borrowed from |
|---|---|
| Fenced, typed directive blocks with YAML bodies | MyST Markdown / MDX |
| Sparse cell targeting (`@ A1 …`) — O(1) per populated cell | SocialCalc's command log |
| Dense pipe-row grids for contiguous data | GFM tables / TOON tabular collapse |
| Explicit formula ⇄ cached-value duality (`=F :: V`) | OOXML's `<f>`/`<v>` pair |
| Escape hatch to raw OOXML/JSON for anything exotic | MDX's JSX / Markdown's HTML fall-through |
| Canonical en-US formula locale on disk | ODF OpenFormula |

One cell edit is one line change. One block is one database row. One file is one
workbook. And a GridMD file renders acceptably in any Markdown viewer — headings
are sheets, dense payloads remain readable as pipe rows, and directives are code
fences.

## A tour, simple to complex

### 1 · The smallest workbook

One sheet, three cells. `@ <ref> <value>` is the whole grammar you need:

```gridmd
---
gridmd: "0.1"
---

# Sheet1

@ A1 "Hello"
@ B1 42
@ C1 =B1*2 :: 84
```

`C1` shows the two-sided nature of a spreadsheet cell: the formula (`=B1*2`)
**and** its cached result after ` :: ` (84), so a reader can display the sheet
without a calculation engine — and a generator that *can't* compute simply
omits the cache rather than guessing.

### 2 · Types without ceremony

Scalars are typed by shape, exactly the way you'd write them:

```gridmd
# Types

@ A1 Plain text          # bare text
@ A2 "TRUE"              # quoted → stays text
@ A3 '0042               # leading apostrophe → forced text, Excel-style
@ B1 -12.5               # number
@ B2 TRUE                # boolean
@ B3 2026-07-04          # date (ISO on disk; serial in .xlsx)
@ B4 12:30               # time
@ B5 #DIV/0!             # a real error value
```

### 3 · Formatting and merges

Properties ride in a `{ … }` map at the end of the line; ranges format in one
stroke; merging is a range property:

```gridmd
# Report

@ A1:D1 { merge: true, align: center, bold: true, fill: "#1F3FA6", color: "#FFFFFF" }
@ A1 "Quarterly Report"
@ B3 45020.5 { numfmt: "$#,##0.00" }
@ A3:A20 { bold: true }
@ B4:B20 =B3*1.04        # relative fill: B5 gets =B4*1.04, and so on
```

### 4 · Dense data: tables

Contiguous data goes in pipe rows. A `{table}` is a real Excel table — name,
banding, filters, total row, and structured references that formulas can use:

````gridmd
# Sales

```{table} Sales at A1
style: medium-2
total:
  total: =SUBTOTAL(109,[total])
cols:
  price: { numfmt: "$#,##0.00" }
---
| item     | qty | price | total            |
| Widget A | 45  | 12.99 | =[@qty]*[@price] |
| Widget B | 12  | 19.50 | =[@qty]*[@price] |
```

@ F1 =SUM(Sales[total])
````

### 5 · The full surface

Conditional formatting, validation, charts, pivots, sparklines, comments —
each is a fenced directive with a YAML body:

````gridmd
# Dashboard

```{cf} B2:B50
- when: "> 1000"
  format: { fill: "#E7F6E7" }
- bars: { color: accent1 }
- icons: 3-arrows
```

```{validation} C2:C50
type: list
values: [Open, Closed, Blocked]
```

```{chart} combo "Revenue vs margin" at E2:L18
series:
  - name: Revenue
    cat: Sales[item]
    val: Sales[total]
    kind: column
    trendline: { type: linear, forecast: { forward: 2 }, r2: true }
  - name: Margin
    val: Sales[margin]
    kind: line
    axis: y2
legend: { position: bottom }
```

```{pivot} ByItem at N2
source: Sales
rows:
  - { field: item }
values:
  - { field: total, agg: sum }
```
````

Every one of these becomes the real thing in `.xlsx` — chartML/ChartEx parts,
pivot caches with refresh-on-load, x14 sparkline groups — and converts back.
The worked example [examples/quarterly-report.gmd](examples/quarterly-report.gmd)
exercises nearly the whole catalog across five sheets.

## When text isn't enough: the escape hatch

GridMD's cardinal rule is **no silent loss**: whatever a converter meets, it
either represents natively, **carries** verbatim, or fails loudly
([INTEROP.md](INTEROP.md) fidelity classes F0–F3). Two mechanisms implement
the "carry" path:

**1 · `fallback:` inside a directive.** When a feature is *mostly*
expressible but has exotic sub-options the grammar doesn't model (say, a
chart with picture-fill series), the directive keeps the readable summary
and attaches the exact source XML. A converter that fully understands the
directive ignores the fallback; one that doesn't re-emits it untouched:

````gridmd
```{chart} column "Revenue" at E2:K16
series:
  - { name: Revenue, cat: Sales[item], val: Sales[total] }
fallback:
  ooxml: |
    <c:chartSpace xmlns:c="…">…the exact original part…</c:chartSpace>
```
````

**2 · `{raw}` blocks for whole foreign parts.** Features with no plain-text
form at all — a VBA project, an OLE object, SmartArt — travel as opaque
package parts, byte-preserved (base64 for binary) and re-emitted into the
`.xlsx` at the exact part path they came from:

````gridmd
```{raw} ooxml part="xl/vbaProject.bin" encoding=base64
UEsDBBQABgAIAAAAIQ…
```
````

So a round trip through GridMD never destroys what it can't yet speak: the
readable 99 % becomes reviewable text, and the rest rides along intact. (The
same mechanism is how the Go/Rust/Swift/Python ports guarantee lossless
round-trips while emitting a leaner native core than the TypeScript
reference — they carry the original document in a custom part,
`customXml/gridmdCarry.xml`.)

Security note: carried parts are data, not trusted instructions — re-emitting
them into a macro-enabled container requires explicit consent, and `part=`
paths are canonicalized against package-part smuggling
([INTEROP.md §5](INTEROP.md)).

## Implementations

Five implementations, one conformance contract
([conformance/README.md](conformance/README.md)): byte-identical canonical
dumps, identical rejection of invalid documents, and dump-stable
`gmd → xlsx → gmd` round trips, all enforced in CI.

| Directory | Language | Notes |
|---|---|---|
| [js/](js/) | TypeScript (Bun) | The semantic reference. 100 % line coverage; npm package `gridmd`; typechecked and declaration-emitted by tsgo. |
| [go/](go/) | Go | `go install ./go/cmd/gridmd`; 99.8 % coverage. |
| [rust/](rust/) | Rust | `cargo build --release` in `rust/`; 95 %+ coverage. |
| [swift/](swift/) | Swift | SPM package (root `Package.swift`): `.library("GridMD")` + `gridmd` CLI. |
| [python/](python/) | Python | `pip install -e python/`; PyYAML as the single dependency. |

```bash
make setup        # install all toolchains' deps
make test         # every implementation's suite
make conformance  # the cross-language gate (all three laws, all implementations)
```

## Spec documents

| File | Contents |
|---|---|
| [SPEC.md](SPEC.md) | The core normative spec: document model, frontmatter, sheets, cell grammar, `@` directives, `{grid}`/`{spill-cache}` blocks, formula canon, canonical form, conformance. |
| [DIRECTIVES.md](DIRECTIVES.md) | The full directive catalog: `{table}`, `{cf}`, `{chart}`, `{pivot}`, `{validation}`, `{filter}`, `{sparklines}`, objects, `{comments}`, `{outline}`, `{page}`, `{query}`, `{script}`, `{slicer}`, `{raw}` and the rest. |
| [FORMATTING.md](FORMATTING.md) | Style properties, number formats, colors and theme references, built-in cell-style and table-style catalogs, icon sets. |
| [INTEROP.md](INTEROP.md) | XLSX ⇄ GridMD feature mapping, fidelity classes, database storage model, diff/merge behaviour, security notes. |
| [examples/quarterly-report.gmd](examples/quarterly-report.gmd) | A thorough worked example: 5 sheets (incl. a chart sheet) exercising nearly every feature. |
| [src/](src/) + [bin/gridmd-lint.js](bin/gridmd-lint.js) | The reference parser + strict-mode linter (Node, one dependency: `yaml`). `npm install && npm test` runs the 60-test suite; `npm run lint:example` validates the worked example. |
| [src/xlsx/](src/xlsx/) + [bin/gridmd2xlsx.js](bin/gridmd2xlsx.js) | The GridMD → XLSX transformer: full-feature emission — the worksheet core (cells, formulas + cached values, styles, merges, tables + sort state, CF, validation, filters, names, freeze, protection, notes, page setup) **plus charts (chartML incl. combo/secondary axis/trendlines/error bars), chart sheets, pivots (refresh-on-load caches), sparklines, table slicers, images, textboxes/shapes, threaded comments, and scenarios**. The only carried-not-native features are the four with no documented OOXML form (queries, scripts, in-cell cell controls, rich-value entities) — preserved in-package in `customXml/gridmdCarry1.xml`, never dropped. `npm run xlsx:example`. |
| [src/xlsx/read.js](src/xlsx/read.js) + [bin/xlsx2gridmd.js](bin/xlsx2gridmd.js) | The XLSX → GridMD importer: reverses the worksheet core natively (cells with date/shared-string/rich-text handling, styles → props, merges, tables + totals/filters/sort, all CF rule kinds, validation, notes, threaded comments, scenarios, sparklines, filters, page setup, names, protection, views); parts not yet reverse-parsed (charts, drawings, pivots, slicers, media) are carried as `{raw}` blocks. Output self-checks against the linter. `npm run roundtrip:example` runs the full loop: the example's 140 defined cells survive `.gmd → .xlsx → .gmd′` exactly, and `.gmd′` lints clean. |

## Design goals (in priority order)

1. **LLM-authorable.** Low nesting, line-oriented, no escape-character hell.
   Formulas are payloads isolated from structural grammar — an Excel formula is
   pasted into a GridMD file verbatim.
2. **Human-parsable.** Readable and hand-editable in a text editor; renders
   passably as Markdown.
3. **Database-friendly.** Block-addressable; a single cell is reachable without
   parsing the whole document; concurrent edits to different blocks merge cleanly.
4. **Concise.** A populated cell costs one line. Dense data costs ~CSV. Empty
   cells cost nothing.
5. **Full Excel fidelity.** Everything in the XLSX feature surface is representable
   — natively where the grammar covers it, via the `{raw}` escape hatch where it
   does not. Nothing is silently dropped.

When goals conflict, the earlier goal wins — except goal 5's *no silent loss*
rule, which is absolute.

## Status

**Version 0.1 — draft.** The grammar is specified and the reference
implementation is **feature-complete in both directions**: every chart family
(classic chartML incl. radar/bubble/stock/-3d; ChartEx treemap/sunburst/
waterfall/funnel/histogram/pareto/box-whisker/map), PivotCharts, pivot
timelines, pivots, sparklines, slicers, images, shapes, threaded comments and
scenarios emit natively AND reverse-parse; the worked example round-trips
`.gmd → .xlsx → .gmd′` with its 140 cells intact; and every cached value is
machine-verified by the bounded formula evaluator (`gridmd-calc`: 13/13, 0
unsupported). Go, Rust and Swift ports plus the Bun/TS/npm packaging are in
flight — see PLAN.md and conformance/README.md. The one check impossible on
this machine remains the **open-in-real-Excel test** (structural verification
only). Naming is provisional: *GridMD* is the spec/working name; *SheetMark*
is the leading marketing-name candidate.

## Non-goals

- Replacing XLSX as the archival/exchange format for existing Excel estates.
- Representing VBA/binary payloads as anything richer than an opaque block.
- Capturing transient application state (window arrangement, cursor position,
  clipboard, undo history).
