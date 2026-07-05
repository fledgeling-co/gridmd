# GridMD Specification

**Version:** 1.0 · **File extension:** `.gmd` · **Media type (proposed):** `text/gridmd`

The key words MUST, MUST NOT, SHOULD, MAY are to be interpreted as in RFC 2119.

---

## 1. Document model

A GridMD document represents exactly one **workbook**:

```
workbook
├── frontmatter            (YAML: identity, calc, theme, names, styles, protection)
├── workbook directives    (fenced blocks before the first sheet: {query}, {script}, {raw}…)
└── sheets                 (one per level-1 heading, in document order)
    ├── {sheet} block      (optional: geometry, view, protection, sheet-scoped names)
    └── content blocks     (any number, in any order)
        ├── @ directives   (single cells, ranges, formatting)
        ├── {grid} blocks  (dense rectangular data)
        ├── {spill-cache} blocks (non-defining dynamic-array display caches)
        └── feature directives ({table}, {chart}, {cf}, {pivot}, …)
```

Sheet order in the workbook **is** document order. Block order within a sheet is
significant only where a directive says so (conditional-format priority, object
z-order fallback); otherwise blocks are an unordered set describing one sheet.

## 2. Lexical rules

- **Encoding:** UTF-8, no BOM. Line ending: LF (`\n`). Writers MUST emit LF;
  readers SHOULD accept CRLF.
- **Blank lines** are insignificant everywhere except inside fenced bodies.
- **Doc comments:** a line beginning with `>` (outside fenced bodies) is a
  human/AI annotation. Parsers MUST ignore it for model purposes and SHOULD
  preserve it adjacent to the following block on round-trip.
- Any other line outside a recognized construct is an **error in strict mode**
  and ignored with a warning in lenient mode (§13).
- Indentation is spaces only. Two spaces per level in multiline `@` bodies.

## 3. Frontmatter (workbook header)

The document MUST begin with a YAML frontmatter block delimited by `---` lines.

```yaml
---
gridmd: "1.0"                       # REQUIRED. Spec version.
title: Q3 Board Pack                # optional workbook title
properties:                         # optional document properties
  author: Luke Rhodes
  company: Diolog
  created: 2026-07-04
  modified: 2026-07-04T09:12:00Z
locale: en-US                       # informative; formulas are ALWAYS canonical en-US (§8)
date-system: 1900                   # 1900 (default) | 1904
calc:
  mode: auto                        # auto | auto-no-tables | manual
  iterative: { enabled: true, max-iterations: 100, max-change: 0.001 }
  precision-as-displayed: false
theme:                              # see FORMATTING.md §4
  colors: { accent1: "#1F3FA6", accent2: "#63BE7B" }   # unlisted slots = Office defaults
  fonts:  { major: Inter, minor: Inter }
names:                              # workbook-scoped defined names
  - { name: TaxRate, ref: "Assumptions!$B$2" }
  - { name: FtoC, formula: "LAMBDA(F,(F-32)*5/9)" }
  - { name: Regions, value: '{"AU","NZ","UK"}' }        # constant
  - { name: _legacy, ref: "Data!$A$1", hidden: true }
styles:                             # named styles; see FORMATTING.md §5
  hdr:   { bold: true, color: "#FFFFFF", fill: accent1 }
  money: { numfmt: "$#,##0.00" }
links:                              # external workbooks referenced by formulas (§8.6)
  - { id: 1, target: "fy25-actuals.xlsx" }
protection:
  structure: true                   # workbook structure lock
  windows: false
  password: { algo: SHA-512, salt: "b64…", spin: 100000, hash: "b64…" }   # optional
---
```

A defined name has exactly one of `ref` (a range/reference), `formula`
(an expression, including `LAMBDA`), or `value` (a constant). Optional keys:
`hidden`, `comment`. Sheet-scoped names live in the `{sheet}` block (§5).

A constant holds the exact en-US text that would follow `=` in Excel's Name
Manager. In YAML, single-quote it so inner double quotes survive verbatim
(double a single quote to embed one):

```yaml
names:
  - { name: Regions, value: '{"AU","NZ","UK"}' }   # array constant — double quotes intact
  - { name: Owner,   value: '"O''Brien"' }         # the text constant "O'Brien"
```

## 4. Sheets

A level-1 ATX heading opens a sheet; its text (trimmed) is the sheet name
verbatim:

```
# Financials
```

Sheet names follow Excel's rules (≤31 chars; no `: \ / ? * [ ]`; unique
case-insensitively). Everything until the next level-1 heading (or EOF) belongs
to this sheet. Headings of level 2+ are treated as doc comments (ignored,
preserved) — use them to organize long sheets for human readers.

## 5. The `{sheet}` block

An optional fenced block immediately configuring the current sheet:

````
```{sheet}
kind: worksheet              # worksheet (default) | chart
tab-color: accent1
hidden: false                # false | true | very
freeze: B3                   # panes frozen above and left of B3 (Excel semantics)
split: D10                   # movable split at D10 (alternative to freeze)
view:
  zoom: 120
  gridlines: true
  headings: true
  formulas: false            # "show formulas" toggle
  rtl: false
  active: B14                # optional active cell
default-row-height: 20       # px
default-col-width: 88        # px
cols:
  A: 140                     # bare number = width px
  "B:D": { width: 96 }
  E: { hidden: true }
  F: { style: money, group: 1 }          # column outline level
rows:
  1: { height: 32 }
  "5:9": { group: 1, hidden: false }
protect:
  enabled: true
  allow: [select-locked, select-unlocked, sort, autofilter]   # see DIRECTIVES.md §16
  password: { algo: SHA-512, salt: "…", spin: 100000, hash: "…" }
names:                        # sheet-scoped defined names
  - { name: PrintBlock, ref: "$A$1:$H$40" }
```
````

All keys optional. Widths/heights are CSS-pixel units (converters map to Excel
character-width/point units; see INTEROP.md §2). A `kind: chart` sheet is an
Excel chart sheet: it does not have a normal editable grid, and its primary
content is a `{chart}` directive anchored `at sheet` (DIRECTIVES.md §5).

## 6. Cell scalars — the content micro-grammar

Wherever a cell's content appears (in `@` directives and grid/table rows), it is
a **scalar** parsed by these rules, applied in order:

| Rule | Content | Parsed as |
|---|---|---|
| 1 | *(empty)* | blank cell |
| 2 | starts `=` | **formula** (§8), with optional cached value: `=SUM(A:A) :: 45020.5` |
| 3 | starts `{=` and ends `}` | legacy CSE **array formula** (same `::` rule) |
| 4 | starts `'` | **text**: everything after the apostrophe, verbatim (Excel parity; forces text) |
| 5 | `"…"` double-quoted | **text**: unquoted content; `""` = literal quote |
| 6 | JSON-grammar number | **number** |
| 7 | `TRUE` / `FALSE` (case-insensitive) | **boolean** |
| 8 | ISO 8601 `YYYY-MM-DD`, `YYYY-MM-DDThh:mm[:ss]`, or `hh:mm[:ss]` | **date/time** (stored ISO; converted to a serial per `date-system` on export) |
| 9 | `#NULL!` `#DIV/0!` `#VALUE!` `#REF!` `#NAME?` `#NUM!` `#N/A` `#GETTING_DATA` `#SPILL!` `#CALC!` `#FIELD!` `#BLOCKED!` | **error** value |
| 10 | anything else | **text**, verbatim (leading/trailing whitespace trimmed) |

Notes:

- **The cached-value separator ` :: `** (space-colon-colon-space) splits a
  formula from its cached result. The split point is the **last** ` :: ` outside
  the formula's double-quoted string literals. The cached side is itself a scalar
  (rules 5–10). Cached values are OPTIONAL everywhere: a writer that has results
  SHOULD emit them (a reader then needs no calc engine to display); a generator
  that cannot compute MUST omit them rather than guess.
- **Numbers** are the shortest decimal string that round-trips the IEEE-754
  double (ECMAScript `Number::toString`). No thousands separators, `.` decimal
  point. Display formatting is entirely the job of `numfmt`.
- Text that *looks like* a number/date/bool/error but must stay text uses rule 4
  or 5: `'0042`, `"TRUE"`.
- Inside **pipe rows** (§7), `|` in content is escaped `\|`; a backslash before
  any other character is literal.

## 7. Dense data — `{grid}` blocks

A `{grid}` block writes a rectangle of cells anchored at its top-left:

````
```{grid} B4
| "Region" | "Q1"  | "Q2"  |
| AU       | 4200  | 4610  |
| NZ       | 1100  | =C6*1.05 :: 1155 |
```
````

- Fence line: `{grid}` + one A1 anchor cell. Rows are **pipe rows**: `|`-delimited,
  leading and trailing pipe required, one sheet row per line, cells positional.
- Every cell is a scalar per §6. An empty cell (`|  |`) leaves that cell blank
  (it does **not** clear an existing value — GridMD describes state, not edits;
  in a well-formed document each cell is defined at most once, §13).
- Grids carry **content only**. Formatting comes from `@` range directives,
  named styles, or column styles. For headered, formatted, filterable data use
  `{table}` (DIRECTIVES.md §1), which shares this row syntax.
- There is no column limit beyond the target grid's (XLSX: 16,384 cols ×
  1,048,576 rows). Multiple grids per sheet are normal; use one per island of
  data.

## 8. Formulas

### 8.1 Canonical locale

Formulas on disk are **always canonical en-US**: function names in English,
`,` argument separator, `.` decimal point, `TRUE`/`FALSE`, A1 references.
Localized display (`;` separators, translated names) is an application-layer
concern. This is a hard rule; it is what makes a `.gmd` file portable across
locales.

### 8.2 References

- A1 style: `B2`, `$B$2`, `B:B`, `2:2`, `B2:D9`.
- Cross-sheet: `Financials!B2`; quoted when the name needs it: `'Q3 Data'!B2`.
- 3-D: `Sheet1:Sheet3!A1`.
- Structured (table) references: `Sales[total]`, `Sales[@qty]`,
  `Sales[[#Totals],[total]]`.
- Spill references: `D2#`.
- External workbook: `[1]Actuals!B2` where `1` is an `id` in frontmatter
  `links:` (§3). Readers without the external file evaluate these to `#REF!` or
  use the cached value.
- R1C1 is **not** a storage format. Converters MUST translate to A1.

### 8.3 Dynamic arrays and spills

A formula that spills declares its (last-known) spill range as a prop:

```
@ D2 =SORT(A2:A9) { spill: D2:D9 }
```

Cached spill values, if kept, are written as a non-defining `{spill-cache}`
block at the same anchor:

````
@ D2 =SORT(A2:A9) { spill: D2:D9 }
```{spill-cache} D2
| AU |
| NZ |
| UK |
```
````

`{spill-cache}` uses the same pipe-row syntax and scalar grammar as `{grid}`,
but its cells are display caches owned by the anchor formula, not independent
cell definitions. In strict mode, the block's anchor MUST match a spilling
formula and its rectangle MUST fit inside the declared spill range. A
`{spill-cache}` cell does not violate the "cell defined at most once" rule.

### 8.4 Legacy array formulas

`{=TRANSPOSE(A1:B5)}` marks a CSE array formula; multi-cell CSE ranges declare
`{ array: D2:E6 }` on the anchor cell. Cached display values for a multi-cell
CSE range use the same `{spill-cache}` mechanism as dynamic spills (§8.3),
anchored on the CSE anchor cell.

### 8.5 Relative fill

An `@` directive whose target is a **range** and whose content is a formula
fills the range with the formula **relatively translated** from the top-left
anchor, exactly as Excel fill does:

```
@ E2:E40 =[@qty]*[@price]
@ B2:B10 =A2*1.1        # B5 gets =A5*1.1
```

This is the token-efficient way to express a formula column.

### 8.6 Verbatim rule

Beyond locale canonicalization, formulas are stored **verbatim as authored**.
Writers MUST NOT reflow whitespace, re-case functions, or rewrite references,
except when translating from a localized or R1C1 source.

## 9. Sparse cells and formatting — `@` directives

The `@` directive is the workhorse for everything outside dense grids.

### 9.1 Single-line forms

```
@ A1 "Q3 Earnings Report"                          # content only
@ B2 =SUM(B4:B10) :: 45020.5                       # formula + cached value
@ B2 =SUM(B4:B10) :: 45020.5 { numfmt: "$#,##0.00", style: money }
@ C7 { fill: "#FDECEC" }                           # props only (format w/o content)
@ A1:D1 { merge: true, align: center, style: hdr } # range props (this is how you merge)
@ B2:B40 { numfmt: "$#,##0.00" }                   # range formatting
@ E2:E40 =[@qty]*[@price]                          # relative fill (§8.5)
```

Grammar: `@` + space + **target** (cell or range, optionally `Sheet!`-qualified
inside that sheet's section only for clarity) + optional **scalar** + optional
**props** as a YAML flow mapping `{ … }` at end of line. To find props, parsers
split from the right at the final balanced flow mapping outside scalar quotes,
formula string literals, Excel structured-reference brackets, and Excel array
constants. If a writer cannot determine that split unambiguously, it MUST use
the multiline form.

### 9.2 Multiline form

When props are numerous or multiline (notes, rich text), indent a YAML mapping
under the `@` line:

```
@ B2
  formula: =SUM(B4:B10)
  value: 45020.5                 # cached value when formula present; content otherwise
  numfmt: "$#,##0.00"
  style: { bold: true }
  link: "https://example.com/q3"
  tip: "Source: board pack"
  note: |
    Includes accruals booked after 28 Jun.
  rich:                          # rich-text runs (overrides plain value for display)
    - { text: "45,020 ", bold: true }
    - { text: "(+12%)", color: "#0B8A00" }
```

### 9.3 Cell/range property vocabulary

All optional. Font/fill/border/alignment details, value syntax and the named
catalogs are specified in FORMATTING.md.

| Key | Applies to | Meaning |
|---|---|---|
| `style` | cell/range | Named style (frontmatter `styles:` or built-in catalog) applied first; explicit props below override it |
| `font`, `size`, `bold`, `italic`, `underline`, `strike`, `sub`, `super`, `color` | cell/range | Font properties (`underline: true \| double \| single-accounting \| double-accounting`) |
| `fill`, `pattern` | cell/range | Background fill color; optional pattern (`gray-125`, `dark-grid`, …) |
| `border`, `border-top/right/bottom/left`, `border-diag-up/down` | cell/range | `"thin #D6D9E0"` shorthand or a map; on a **range**, `border` means outline + optional `border-inner` |
| `align`, `valign`, `rotation`, `indent`, `wrap`, `shrink` | cell/range | Alignment: `align: left\|center\|right\|justify\|fill\|center-across\|distributed`; `rotation: -90..90 \| vertical` |
| `numfmt` | cell/range | Excel number-format code or a built-in alias (FORMATTING.md §2) |
| `merge` | range | `true` — merge the range (content/props of the top-left cell rule) |
| `locked`, `hidden` | cell/range | Protection flags (`hidden` hides the formula when the sheet is protected) |
| `link`, `tip` | cell | Hyperlink (`https:`/`mailto:`/`#Sheet!A1` internal) + tooltip |
| `note` | cell | Legacy note (yellow-box annotation; distinct from threaded `{comments}`) |
| `rich` | cell | Rich-text runs, each `{ text, …font props }` |
| `spill`, `array` | cell | Dynamic-array spill range / legacy CSE range (§8.3–8.4) |
| `control` | cell | In-cell control: `checkbox` (boolean cells) |
| `entity`, `fields` | cell | Rich data type (stock/geography/currency) — DIRECTIVES.md §14 |
| `x-*` | any | Extension namespace; MUST round-trip untouched |

### 9.4 Definition vs annotation

A directive **defines** a cell when it gives it content: an inline scalar, a
`value:`/`formula:`/`rich:`/`entity:` key in a multiline body, or a non-empty
payload cell of a `{grid}` or `{table}`. Everything else — props-only `@` directives,
`note:`/`link:`/`tip:`, protection flags, `{spill-cache}` cells — is an
**annotation** and may target defined or undefined cells freely. The
define-once rule (§12.5, §13) counts definitions only; overlapping annotations
compose per the precedence rules in FORMATTING.md §8. A range target with
non-formula inline content is an error (relative fill, §8.5, is the only
range-content form).

## 10. Directive blocks (general form)

All non-`@` features are fenced directive blocks:

````
```{kind} positional-args key=val …
<optional YAML meta>
---
<optional payload rows / body>
```
````

- The fence line: 3+ backticks, `{kind}`, then positional arguments (anchor,
  name, type — per directive), then optional `key=val` flags.
- The body is YAML unless the directive defines a payload section; where both
  exist they are separated by a line containing only `---`.
- **Anchors:** `at B2` (cell), `at B2:K18` (two-cell anchor — the object
  stretches with the grid), `at B2 size 480x320` (one-cell anchor + fixed px
  size), `at 120,80 size 480x320` (absolute px), or `at sheet` for the single
  chart on a `kind: chart` sheet. Optional `offset: [x, y]` px in the body.
  Anchors are sheet-local: an optional `Sheet!` qualifier MUST name the
  containing sheet (it is documentation, not cross-sheet placement).
- Unknown `{x-*}` directives MUST round-trip untouched. Unknown non-`x-`
  directives are an error in strict mode.
- **YAML flow-context gotcha:** inside flow collections (`{ … }` / `[ … ]`)
  YAML reserves `[ ] { } ,`, so structured references and array constants
  must be quoted there (`val: "Sales[total]"`). In block form (one key per
  line) they may stay plain (`val: Sales[total]`). Writers SHOULD prefer
  block form for any value carrying brackets.

The full catalog is in DIRECTIVES.md. Reserved kinds in 0.1: `sheet`, `grid`,
`spill-cache`, `table`, `cf`, `validation`, `filter`, `chart`, `sparklines`,
`pivot`, `slicer`, `image`, `shape`, `textbox`, `checkbox`, `comments`,
`outline`, `page`, `query`, `script`, `scenario`, `raw`.

## 11. The escape hatch

Fidelity is absolute: what the grammar can't say natively is carried, never
dropped.

1. **In-directive fallback.** Any directive body MAY carry a `fallback:` key
   holding the exact source representation; a converter that fully understood
   the directive ignores it, one that didn't emits it back out:

   ```yaml
   fallback:
     ooxml: |
       <c:chartSpace …>…</c:chartSpace>
   ```

2. **Standalone `{raw}` blocks** carry whole foreign parts (workbook- or
   sheet-level):

   ````
   ```{raw} ooxml part="xl/vbaProject.bin" encoding=base64
   AAABBBCCC…
   ```
   ````

See DIRECTIVES.md §18 and INTEROP.md §1–§2 for the fidelity classes,
package-safety rules, and re-emission rules.

## 12. Canonical form

Two semantically identical workbooks should serialize byte-identically, so
diffs and content hashes are meaningful. A **canonical GridMD writer**:

1. Emits frontmatter keys in the order of §3; omits keys at default values.
2. Emits sheets in workbook order; within a sheet: the `{sheet}` block, then
   `{table}`/`{grid}` blocks by anchor (row-major), then `@` content cells by
   address (row-major), with any `{spill-cache}` immediately after its owning
   spilling formula, then `@` formatting ranges, then feature directives in the
   catalog order of DIRECTIVES.md. Directives are sorted by anchor/target unless
   their semantics depend on order; order-sensitive directives such as `{cf}` and
   same-`z` objects MUST preserve effective order or serialize explicit ordering
   keys before being reordered.
3. Uses single-line `@` when the directive fits in 100 columns, multiline
   otherwise.
4. Emits numbers per §6, colors lowercase hex, YAML flow maps with a single
   space after `:` and `,`.
5. Never emits a cell twice (last-write-wins is only a lenient-reader rule).
6. Prefers content in `{table}`/`{grid}` blocks when ≥2 contiguous rows × 2
   columns are populated; `@` otherwise.

Readers MUST accept non-canonical documents; `gridmd fmt` (a formatter) is the
expected normalizer, exactly as `prettier`/`gofmt` are for code.

## 13. Conformance

- **Strict mode** (default for machine pipelines): unknown constructs,
  duplicate cell definitions, out-of-bounds references, malformed scalars, and
  orphaned or out-of-range `{spill-cache}` blocks are errors.
- **Lenient mode** (default for interactive/AI ingestion): unknown lines are
  skipped with warnings; duplicate cell definitions resolve last-write-wins;
  orphaned or out-of-range `{spill-cache}` blocks are skipped with warnings;
  a missing cached value is never an error.
- A **reader** MUST: parse §2–§10; honor `::` cached values without a calc
  engine; preserve `x-*` and `{raw}`/`fallback` payloads.
- A **writer** MUST: emit canonical en-US formulas; never fabricate cached
  values; never silently drop a feature (represent natively, or via §11, or
  fail loudly).
- A **calc-capable application** SHOULD recompute on load and MAY rewrite
  cached values; a non-calc application MUST leave them untouched.

## 14. Embedding

A complete GridMD document may be embedded in ordinary Markdown/MDX inside a
` ```gridmd ` fence. Renderers without GridMD support show readable source;
GridMD-aware renderers may render a live sheet.

---

## Appendix A — grammar (informative EBNF)

```ebnf
document      = frontmatter , { wb-block } , { sheet } ;
frontmatter   = "---" NL , yaml-lines , "---" NL ;
wb-block      = fenced | doc-comment | blank-line ;
sheet         = "#" SP sheet-name NL , { block } ;
block         = at-directive | fenced | doc-comment | blank-line ;
doc-comment   = ">" [ text ] NL ;              (* also headings of level 2+ *)

(* ---- @ directives ---- *)
at-directive  = "@" SP target [ SP content ] NL , [ at-body ] ;
target        = [ sheet-name "!" ] ( cell | range ) ;
cell          = [ "$" ] col [ "$" ] row ;
range         = cell ":" cell | col ":" col | row ":" row ;
col           = "A" … "XFD" ;                  (* uppercase; 1..16384 *)
row           = "1" … "1048576" ;
content       = scalar [ SP props ] | props ;
props         = flow-map ;                     (* see the props rule below *)

(* ---- fenced directives ---- *)
fenced        = fence-open NL , fence-body , fence-close ;
fence-open    = ticks "{" kind "}" { SP arg } ;
ticks         = "`" "`" "`" { "`" } ;
fence-close   = ticks-ge-open { SP } NL ;      (* >= opening tick count, nothing else *)
kind          = ident | "x-" ident ;
arg           = anchor | size | flag | positional ;
anchor        = "at" SP ( "sheet" | cell | range | int "," int ) ;
size          = "size" SP int "x" int ;
flag          = ident "=" ( bare-token | dq-string ) ;
positional    = bare-token | dq-string ;
dq-string     = '"' { char - '"' | '""' } '"' ;   (* "" = literal quote *)
fence-body    = [ yaml-lines ] , [ "---" NL payload-lines ] ;
payload-lines = { pipe-row NL } | code-lines ; (* pipe rows, except {script}: code *)
pipe-row      = "|" { cell-text "|" } ;        (* backslash escapes the next char; \| = pipe *)

(* ---- cell scalars (§6) ---- *)
scalar        = formula | cse-formula | tick-text | dq-string | number
              | boolean | iso-datetime | error | bare-text | empty ;
formula       = "=" ftext [ " :: " scalar ] ;  (* split: LAST " :: " outside "…" in ftext *)
cse-formula   = "{=" ftext "}" [ " :: " scalar ] ;
tick-text     = "'" any-text ;
number        = json-number ;
boolean       = "TRUE" | "FALSE" ;             (* case-insensitive *)
iso-datetime  = date [ "T" time ] | time ;
date          = 4digit "-" 2digit "-" 2digit ;
time          = 2digit ":" 2digit [ ":" 2digit ] ;
error         = "#" ERRNAME ;                  (* the closed set in §6 rule 9 *)
```

Three rules that EBNF alone cannot carry:

1. **`at-body` termination (the dedent rule).** The body of a multiline `@`
   directive is the maximal run of following lines that are blank or indented
   by ≥ 2 spaces. It ends at the first non-blank line with indent < 2 (or EOF);
   trailing blank lines belong to the document, not the body. The two-space
   indent is stripped and the remainder is parsed as a YAML mapping. Blank
   lines inside the body are preserved (they matter inside block scalars like
   `note: |`).
2. **The props split (right-edge rule).** On a single-line `@` directive, the
   props candidate is the last `{…}` group that (a) is brace-balanced scanning
   left-to-right while ignoring content inside double quotes, (b) extends to
   the end of the line, and (c) is preceded by whitespace. It is accepted as
   props only if it parses as a YAML flow mapping in which every top-level
   entry is `key: value` with an identifier key (`ident` or `x-ident`) and a
   non-null value; otherwise it is part of the scalar (e.g. a trailing Excel
   array constant). A writer that cannot make the split unambiguous MUST use
   the multiline form.
3. **Fence closing.** A fence closes at the first line consisting of only
   backticks (count ≥ the opening count) plus optional trailing spaces. Fence
   bodies never nest fences of an equal-or-smaller tick count; writers
   embedding backtick-bearing payloads use a longer opening fence.

The normative prose (§2–§10) prevails over this appendix.
