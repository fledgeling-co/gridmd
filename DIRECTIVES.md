# GridMD Directive Catalog

**Version 1.0.** Normative companion to [SPEC.md](SPEC.md) §10. Every
catalog directive follows the general fenced form: fence line (`{kind}` +
positional args + `key=val` flags), YAML meta, optional `---`-separated payload.
Core directives `{sheet}`, `{grid}`, and `{spill-cache}` are specified in
SPEC.md.

Catalog order (also the canonical emission order, SPEC §12):
`{table}` · `{cf}` · `{validation}` · `{filter}` · `{chart}` · `{sparklines}` ·
`{pivot}` · `{slicer}` · objects (`{image}` `{shape}` `{textbox}` `{checkbox}`) ·
`{comments}` · `{outline}` · `{page}` · `{query}` · `{script}` · `{scenario}` ·
`{raw}`.

---

## 1. `{table}` — structured tables

A named table object over a range (Excel ListObject): header row, banding,
total row, per-column typing/format, filters, and structured references.

````
```{table} Sales at A4
style: medium-2                 # built-in (FORMATTING.md §6) or a custom style name
header: true                    # default true; false = headerless table
banded: rows                    # rows | cols | both | none (default rows)
total:                          # total row: column name -> formula or label
  product: "Total"
  total: =SUBTOTAL(109,[total])
cols:                           # per-column props, keyed by header name
  qty:   { numfmt: "0" }
  price: { numfmt: "$#,##0.00" }
  total: { numfmt: "$#,##0.00" }
filter:                         # active AutoFilter state — keyed by column NAME
  product: { values: [Widget A, Widget B] }   # (operator vocabulary of {filter} §4)
---
| id | product  | qty | price | total            |
| 1  | Widget A | 45  | 12.99 | =[@qty]*[@price] |
| 2  | Widget B | 12  | 19.50 | =[@qty]*[@price] |
| 3  | Widget C | 89  | 5.00  | =[@qty]*[@price] |
```
````

- Fence line: table **name** (must be a valid Excel table name, unique in the
  workbook) and the **anchor** (top-left cell including the header row).
- The first payload row is the header when `header: true`. Header cells MUST
  be non-empty **text** scalars (quote anything that would otherwise parse as
  a number/date/boolean; formulas are not allowed in headers) and unique
  case-insensitively within the table — duplicates are strict-mode errors.
  Converters importing an XLSX with colliding normalized names disambiguate
  Excel-style by appending `2`, `3`, …. Formulas reference columns as
  `Sales[qty]` / `[@qty]`.
- The table's range is derived: anchor + payload extent (+ total row if
  present). Auto-expand is application behaviour, not serialized state.
- A calculated column is expressed naturally: every body cell in the column
  carries the same relative formula (writers SHOULD emit each row; readers MAY
  compress identical relative formulas on ingest).
- Table `filter:` and `sort:` use the operator vocabulary of `{filter}` (§4)
  but are keyed by **column name** with no `cols:` wrapper, and `sort[].col`
  takes a column name. Column letters are for plain-range `{filter}` only.

## 2. `{cf}` — conditional formatting

One or more blocks may target the same or overlapping ranges; the body is an
ordered rule list. By default, **document order is priority order** (first rule =
highest priority, Excel priority 1). For exact XLSX round-trip, any rule MAY
carry `priority: <positive integer>`; lower numbers have higher priority.

````
```{cf} C3:C8
- when: "> 500"
  format: { fill: "#E7F6E7", color: "#0B8A00" }
- when: "between 100 and 500"
  style: neutral
- contains: "overdue"
  format: { fill: "#FDECEC" }
  stop: true                      # stop-if-true
- top: 10%                        # also: top: 5, bottom: 3, bottom: 20%
  format: { bold: true }
- avg: above                      # above | below | above-equal | below-equal
  stddev: 1                       # optional N std-devs
  format: { italic: true }
- dupes: true                     # duplicate values (unique: true for the inverse)
  format: { fill: "#FFF4E5" }
- date: last-7-days               # yesterday|today|tomorrow|last-7-days|this-week|
  format: { fill: "#EEF1F6" }     # last-week|next-week|this-month|last-month|next-month
- bars:                           # data bars
    color: accent1
    gradient: true
    min: { type: auto }           # auto|min|max|number|percent|percentile|formula + value
    max: { type: percentile, value: 95 }
    negative: { color: "#C0392B", axis: middle }
- scale: ["#F8696B", "#FFEB84", "#63BE7B"]   # 2- or 3-color scale, min→max
  stops: [ {type: min}, {type: percentile, value: 50}, {type: max} ]   # optional
- icons: 3-arrows                 # catalog: FORMATTING.md §7
  steps:                          # optional custom thresholds (ascending)
    - { op: ">=", value: 67, type: percent }
    - { op: ">=", value: 33, type: percent }
  reverse: false
  icons-only: false               # show icon, hide value
- formula: =MOD(ROW(),2)=0        # "format where formula is true"
  format: { fill: "#F7F8FB" }
  # priority: 12                  # optional explicit Excel priority
```
````

- Rule type is inferred from its distinguishing key (`when`, `contains`,
  `not-contains`, `begins`, `ends`, `date`, `dupes`, `unique`, `top`, `bottom`,
  `avg`, `bars`, `scale`, `icons`, `formula`).
- `when` operators: `= <> > >= < <=`, `between A and B`, `not-between A and B`.
  Operands are scalars or `=formula`.
- Each rule takes `format:` (inline props: font/fill/border/numfmt subset) or
  `style:` (named), plus `stop: true`.
- Multiple `{cf}` blocks may target overlapping or identical ranges. If no
  explicit priorities are present, priority runs block-order then rule-order.
  If any rule on a sheet has `priority`, readers sort explicit priorities
  ascending and then append unprioritized rules in document order. Writers
  importing XLSX SHOULD emit explicit `priority` when document order alone would
  not preserve the source workbook's rule ordering — and SHOULD then emit it on
  **all** of that sheet's rules, not a mix (mixed mode demotes every
  unprioritized rule below every explicit one). Canonical writers (SPEC §12)
  preserve effective document order and emit explicit priorities only when the
  source carried them or document order cannot express the ordering.

## 3. `{validation}` — data validation

````
```{validation} A2:A100
type: list                        # list|whole|decimal|date|time|text-length|custom
values: [Open, Closed, Blocked]   # list: inline values…
# source: =Lists!$A$1:$A$10       # …or a range/formula
dropdown: true                    # show in-cell dropdown (list only; default true)
blank: true                       # ignore blank (default true)
input:  { title: "Status", message: "Pick one of the allowed states." }
error:  { style: stop, title: "Invalid", message: "Choose a value from the list." }
```                               # error.style: stop | warning | information
````

Non-list types use `op` + operands: `op: between, min: 1, max: 100` or
`op: ">=", value: =TODAY()`. `type: custom` takes `formula: =…`.

## 4. `{filter}` — AutoFilter + sort state on a plain range

(For tables, put the same `filter:`/`sort:` schema in the `{table}` meta.)

````
```{filter} A1:F200
cols:                             # keyed by column LETTER for plain ranges
  B: { values: [Open, Blocked] }              # value checklist
  D: { op: ">", value: 100 }                  # condition (ops: = <> > >= < <=,
  E: { op: contains, value: "widget" }        #  begins, ends, contains, not-contains)
  F: { top: 10 }                              # top/bottom N or N%
  C: { fill: "#FFEB84" }                      # by cell color; font: / icon: also allowed
sort:                             # last-applied sort state (optional, informative)
  - { col: D, order: desc, by: value }        # by: value | fill | font | icon
  - { col: B, order: asc, by: value }
headers: true
```
````

## 5. `{chart}` — charts

````
```{chart} column "Revenue by product" at G2:N20
data: Sales[product], Sales[total]        # shorthand: categories, then 1+ value ranges
```
````

The shorthand covers the common case; its `data:` list splits on commas
**outside brackets**, so structured references that contain commas
(`Sales[[#Totals],[total]]`) stay intact. The full form:

````
```{chart} combo "Revenue vs margin" at G2:N24
series:
  - name: Revenue                 # or name-ref: =Summary!$B$1
    cat: Sales[product]
    val: Sales[total]
    kind: column                  # per-series override (combo charts)
    color: accent1
    fill: { type: solid, color: accent1 }     # solid|gradient|pattern|picture|none
    outline: { color: "#0F1A2E", width: 1, dash: solid }
    labels: { show: true, position: outside-end, contains: [value], numfmt: "$#,##0" }
    gap: 60                       # gap width %
    overlap: -10                  # series overlap %
    trendline:
      type: linear                # linear|poly|exp|log|power|moving-average
      order: 2                    # poly only
      window: 3                   # moving-average only
      forecast: { forward: 2, backward: 0 }
      intercept: 0                # optional set-intercept
      equation: true              # display equation on chart
      r2: true                    # display R²
    error-bars:
      dir: both                   # both|plus|minus
      type: std-error             # std-error|percentage|std-dev|fixed
      value: 5                    # percentage/fixed/std-dev amount
      cap: true
  - name: Margin %
    val: Sales[margin]
    kind: line
    axis: y2                      # plot on secondary axis
    smooth: true
    marker: circle
axes:
  x:  { title: "Product", labels: true, gridlines: false }
  y:  { title: "Revenue", min: 0, max: 60000, unit: 10000, minor-unit: 2500,
        ticks: out, minor-ticks: none, gridlines: true, numfmt: "$#,##0,K",
        log: false, reverse: false, crosses: auto }
  y2: { title: "Margin", numfmt: "0%", max: 1 }
legend: { position: bottom, overlay: false }    # left|right|top|bottom|none
data-table: { show: false, legend-keys: true }
style: { palette: colorful-2 }    # Change Colours gallery; or per-series color
switch-row-col: false
alt: "Column chart of revenue by product with margin line."
```
````

- **Types:** `column bar line area pie doughnut scatter bubble radar stock
  surface histogram pareto box-whisker treemap sunburst waterfall funnel map
  combo`, with `-stacked` / `-stacked100` / `-3d` suffixes where Excel has them
  (`column-stacked`, `bar-stacked100`, `pie-3d`…). A converter meeting an
  unsupported subtype MUST use `fallback:` (§18), never drop the chart.
- **PivotCharts:** replace `series`/`data` with `pivot: <pivot-name>`.
- A chart on its own chart-sheet: anchor `at sheet` inside a sheet section whose
  `{sheet}` block has `kind: chart` (SPEC.md §5 and §10). A chart sheet MUST have
  exactly one primary `{chart}` anchored `at sheet`; additional worksheet-grid
  content is an error in strict mode.

## 6. `{sparklines}` — in-cell mini charts

````
```{sparklines} F2:F13
type: line                        # line | column | win-loss
source: B2:E13                    # one source row/col per target cell, in order
markers: { high: true, low: true, first: false, last: false, negative: true }
color: accent1
axis: { min: auto, max: auto, show: false }
```
````

## 7. `{pivot}` — pivot tables

````
```{pivot} RevenueByRegion at Summary!A12
source: Sales                     # table name or range (Data!A1:F500)
rows:
  - { field: region, sort: asc, subtotal: true }
  - { field: product, sort: by-value, using: "Sum of total", order: desc }
cols:
  - { field: quarter }
values:
  - { field: total, agg: sum, name: "Sum of total", numfmt: "$#,##0" }
  - { field: total, agg: count, name: "Deals" }
  - { field: total, agg: sum, show-as: percent-of-column, name: "% of column" }
filters:
  - { field: status, selected: [Closed] }
layout: compact                   # compact | outline | tabular
grand-totals: { rows: true, cols: true }
blank-rows: false
refresh: on-load                  # on-load | manual
```
````

- `agg`: `sum count average max min product count-numbers std-dev std-devp var
  varp`.
- `show-as`: `percent-of-total percent-of-row percent-of-column percent-of
  running-total difference-from percent-difference-from rank-asc rank-desc
  index` (+ `base-field` / `base-item` where required).
- GridMD does **not** serialize the pivot cache records — the source data is in
  the document; `refresh: on-load` is the default contract. A converter that
  must preserve an exotic OLAP-backed pivot uses `fallback:`.

## 8. `{slicer}` — slicers & timelines

````
```{slicer} at H2 size 160x220
for: Sales                        # table or pivot name
field: product
selected: [Widget A, Widget B]    # omit = no filter
multi: true
```
````

A timeline is `{slicer}` with `kind: timeline` and `field:` a date field, plus
`level: days|months|quarters|years` and `range: [2026-01-01, 2026-06-30]`.

## 9. Objects — `{image}`, `{shape}`, `{textbox}`, `{checkbox}`

All floating objects share: an anchor (SPEC §10), optional `name:`, `z:`
(integer; higher = front; ties break by document order), `locked:`, `alt:`.

````
```{image} at D2 size 240x120
src: assets/logo.png              # relative path, https: URL, or data: URI
alt: "Company logo"
```

```{shape} rounded-rect at F2:H5
text: "Draft — do not circulate"
fill: "#FFF4E5"
outline: { color: "#B45309", width: 1 }
font: { bold: true, color: "#B45309", align: center, valign: middle }
```

```{textbox} at B20 size 320x80
text: |
  Assumes FX held at the 30-Jun rate.
font: { size: 11, color: "#5E6A82" }
```

```{checkbox} at C4
label: "Include forecast"
linked: $D$4                      # linked cell receives TRUE/FALSE
checked: true
```
````

- **Shape kinds:** `rect rounded-rect ellipse triangle right-triangle diamond
  pentagon hexagon star arrow-right arrow-left arrow-up arrow-down chevron
  callout line connector` (the common gallery). Exotic preset geometry rides
  `fallback:`.
- **In-cell images** ("place in cell") are not objects: use the `IMAGE()`
  formula or a cell `entity` of type `image`.
- **In-cell checkboxes** are the cell prop `control: checkbox` (SPEC §9.3);
  the `{checkbox}` directive is the floating form control.
- Ink/drawing layers, SmartArt, WordArt beyond `{textbox}` presets, and OLE
  objects are escape-hatch classes (INTEROP.md §2).

## 10. `{comments}` — threaded comments

Threaded, multi-author comments (distinct from `note:` on a cell, SPEC §9.3).

````
```{comments} B7
- by: "Priya N"
  at: 2026-07-02T09:14:00Z
  text: "Does this include accruals?"
  resolved: true
  replies:
    - { by: "Luke R", at: 2026-07-02T09:31:00Z, text: "Yes — see the cell note." }
- by: "Luke R"
  at: 2026-07-03T14:02:00Z
  text: "@Priya re-check after the July close."
```
````

## 11. `{outline}` — grouping & subtotal structure

Row/column grouping beyond the simple `group:` levels in `{sheet}` `rows:`/`cols:`
(use whichever is clearer; they are equivalent):

````
```{outline}
rows:
  - { range: "5:9",  level: 1, collapsed: false }
  - { range: "6:7",  level: 2, collapsed: true }
cols:
  - { range: "D:F", level: 1 }
summary: below-right              # below-right (default) | above-left
```
````

Subtotal rows produced by Data ▸ Subtotal are ordinary `SUBTOTAL()` formulas in
the grid plus this outline structure — nothing more is serialized.

## 12. `{page}` — page setup & print

````
```{page}
orientation: landscape            # portrait | landscape
paper: A4                         # A3 A4 A5 letter legal tabloid …
margins: { top: 1.9, bottom: 1.9, left: 1.8, right: 1.8, header: 0.8, footer: 0.8 }  # cm
scale: 100                        # % — or:
# fit: { width: 1, height: 0 }    # fit-to (0 = automatic)
print-area: A1:H40
print-titles: { rows: "1:1", cols: "A:A" }
breaks: { rows: [20, 40], cols: [] }
header: { left: "&D", center: "&B Q3 Board Pack", right: "" }
footer: { right: "Page &P of &N" }
gridlines: false                  # print gridlines
headings: false                   # print row/col headings
center: { horizontal: true, vertical: false }
first-page-number: auto
black-and-white: false
```
````

Header/footer strings use Excel's `&` codes (`&P` page, `&N` pages, `&D` date,
`&T` time, `&F` file, `&A` sheet, `&B`/`&I`/`&U` styling, `&"font,style"`,
`&&` literal ampersand).

## 13. `{query}` — data queries (bounded transform pipeline)

A declarative source-plus-steps pipeline (a portable, bounded subset of the
Power Query idea — **not** the M language). Workbook-level (before the first
sheet) or sheet-level.

````
```{query} FxRates
source: { url: "https://api.example.com/fx.csv", format: csv }
# source: { file: "rates.xlsx", sheet: "Rates" } | { table: RawRates } | { range: Data!A1:D500 }
steps:
  - promote-headers: true
  - filter: { col: currency, in: [USD, EUR, GBP] }
  - rename: { col: rate_mid, to: rate }
  - remove: [source_ts]
  - type:  { col: rate, as: number }
  - sort:  { col: currency, order: asc }
output: { table: FxRates, at: Data!A1 }
refresh: manual                   # manual | on-open | { every: 30m }
```
````

Step vocabulary (0.1): `promote-headers filter rename remove keep type sort
dedupe split merge-cols add-col` (with `add-col: { name, formula }` using the
formula canon). A query beyond this vocabulary rides `fallback:` (e.g. the
original M text) — represented, refresh-disabled, never dropped.

## 14. Rich data types (stocks / geography / currencies)

A linked-entity cell stores the entity identity plus cached fields; field
access uses Excel's dot syntax (`=B2.Price`).

```
@ B2
  entity: { type: stock, provider: default, id: "XNAS:MSFT", text: "MSFT" }
  fields: { Price: 442.10, Currency: "USD", "Change %": 0.0121 }   # cached snapshot
```

`type: geography | stock | currency | image | custom`. The `fields` map is a
cache with the same honesty rule as `::` values: never fabricated, refreshed by
capable applications.

## 15. `{script}` — automation

````
```{script} normalize-headers lang=js
on: manual                        # manual (default) | open
---
export default function run(wb) {
  const sheet = wb.sheet("Data");
  // …workbook API is host-defined
}
```
````

Scripts are **inert by default**: a conforming reader MUST NOT execute a script
without explicit host policy/user consent (INTEROP.md §5). Recorded macros are
scripts; VBA projects are binary and ride `{raw}`.

## 16. Protection reference

- **Workbook:** frontmatter `protection:` (SPEC §3).
- **Sheet:** `{sheet}` `protect:` with `allow:` drawn from: `select-locked
  select-unlocked format-cells format-columns format-rows insert-columns
  insert-rows insert-hyperlinks delete-columns delete-rows sort autofilter
  pivot-tables objects scenarios`.
- **Cell:** `locked:` (default true — effective only when the sheet is
  protected) and `hidden:` (hide formula).
- Password hashes use the OOXML scheme (`algo`/`salt`/`spin`/`hash`). GridMD
  protection is an editing-courtesy lock, exactly as in Excel — it is **not**
  encryption or access control.

## 17. `{scenario}` — what-if scenarios

````
```{scenario} Downside
cells: { B2: 0.05, B3: 41000 }    # changing cells -> scenario values
comment: "Rates +200bp, volume -10%"
```
````

## 18. `{raw}` — the escape hatch

Carries any foreign payload verbatim (SPEC §11). Placement scope: workbook
(before the first sheet) or the current sheet.

````
```{raw} ooxml part="xl/charts/chart2.xml"
<c:chartSpace xmlns:c="…">…</c:chartSpace>
```

```{raw} ooxml part="xl/vbaProject.bin" encoding=base64
UEsDBBQABgAIAAAAIQ…
```
````

- `format` (first positional): `ooxml | json | text`. `encoding=base64` for
  binary. `part=` names the source part for round-trip re-assembly.
- For `ooxml` raw blocks, `part=` is a ZIP member path, not a filesystem path.
  Writers MUST canonicalize it before use: no leading slash, no backslashes, no
  empty segment, no `.` or `..` segment, no control characters, and no
  percent-encoded traversal. Writers MUST NOT let `{raw}` overwrite
  `[Content_Types].xml`, package relationship parts (`_rels/.rels` or
  `*.rels`), or another native GridMD-emitted part unless the converter is doing
  a whole-package validated reassembly.
- Readers MUST preserve `{raw}` blocks byte-exactly. Writers converting **to**
  XLSX re-emit them into the named parts only after the package-safety checks
  above; writers converting to other targets keep them attached.
- Macro-bearing, ActiveX, OLE, external-relationship, and executable-adjacent
  parts (for example `xl/vbaProject.bin`) require explicit host policy and user
  consent before they are re-emitted to an executable container such as `.xlsm`.
- The same rule applies to the `fallback:` key inside any directive: understood
  → ignored; not understood → re-emitted.
