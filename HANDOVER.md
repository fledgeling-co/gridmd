# Handover: reviewing the GridMD spec

**Audience:** an AI (or human) reviewer picking up this spec cold.
**Task:** adversarially review the GridMD 0.1 draft in this directory with the
same context its author had. Everything you need is in this file plus the five
spec documents; source materials are listed with paths but summarized here in
case they're unavailable.

---

## 1. What GridMD is and why it exists

GridMD is a **Markdown-analogous plain-text serialization format for
full-fidelity spreadsheets** — "Markdown for Excel". It was designed for an
AI-native product context where spreadsheets must be:

1. **LLM-authorable** — generated and read reliably by language models, without
   escape-character hell (the #1 goal; wins ties).
2. **Human-parsable** — hand-editable, renders passably in a Markdown viewer.
3. **Database-friendly** — block-addressable, single-cell reachable, concurrent
   edits merge cleanly.
4. **Concise** — one populated cell ≈ one line; dense data ≈ CSV cost; empty
   cells cost nothing.
5. **Full Excel fidelity** — the entire XLSX feature surface representable;
   the *no-silent-loss* rule is absolute even though fidelity ranks last.

The strategic judgment (stated to the user, accepted): this format is worth
building as a **product-internal representation**, not as a public standard
play — formats win by riding an app. Review it as production infrastructure,
not as an academic exercise.

## 2. Deliverables under review

| File | Role |
|---|---|
| `README.md` | Pitch, quick example, goals, doc map |
| `SPEC.md` | Core normative spec: document model, lexical rules, frontmatter, sheets, cell scalar grammar, formulas, `@` directives, `{grid}`/`{spill-cache}`, escape hatch, canonical form, conformance, mini-EBNF |
| `DIRECTIVES.md` | Catalog of feature directives (table, cf, validation, filter, chart, sparklines, pivot, slicer, objects, comments, outline, page, query, script, scenario, raw); core `{sheet}`/`{grid}`/`{spill-cache}` live in SPEC.md |
| `FORMATTING.md` | Property value syntax, numfmt aliases, colors/theme, built-in cell-style/table-style/icon-set catalogs, precedence rules |
| `INTEROP.md` | Fidelity classes F0–F3, XLSX⇄GridMD feature map, unit conversions, DB storage model, diff/merge, security, versioning |
| `examples/quarterly-report.gmd` | 4-sheet worked example intended to exercise nearly every feature — **validate it line-by-line against the spec; it was hand-written and is the most likely home of inconsistencies** |

## 3. Source materials (the context the author had)

### 3.1 Deep-research report — *"Markdown for Excel: Architectural Design and Prior-Art Analysis"*

Path: `/Users/lukerhodes/Downloads/Markdown For Excel Format Research.md`.
Key findings the design leans on (with the report's confidence levels):

- **XML standards (OOXML/ISO 29500, ODF/ISO 26300)**: 100 % fidelity, hostile
  to LLM token budgets; formulas stored as `<f>` + cached `<v>` pairs; spills
  via `t="array"` + `ref`; CF rules decoupled from styles via `dxfId`; pivots
  as a cache-records/cache-definition/table-definition triad; charts as
  separate multi-thousand-line XML parts. (High confidence)
- **LLM serialization benchmarks**: Markdown-KV 60.7 % extraction accuracy vs
  JSON 52.3 % vs CSV 44.3 % (GPT-4.1-nano, 1,000 synthetic records); TOON
  (indentation + tabular array collapse with header + length prefix) cuts ~40 %
  of JSON's tokens at >76 % generative accuracy. Readable structure beats raw
  density. (High confidence)
- **Delimiter robustness is the critical LLM-ergonomics factor**: Excel
  formulas natively contain commas, double quotes, brackets; embedding them in
  JSON/CSV forces escaping that disrupts BPE token boundaries and drives
  syntax errors. Indentation, multiline fences (Bruno `'''`), and Markdown
  pipes isolate the formula payload. (High confidence)
- **SocialCalc's event-sourced command log** (`set A1 value n 42`): O(1) sparse
  representation, perfect line diffs; weak at deep object hierarchies
  (charts/pivots). (Medium confidence)
- **Grist**: workbook decomposed into SQLite relations — proves the
  hybrid spreadsheet/database storage model. **Bruno .bru**: Git-friendly
  block DSL. **MyST**: fenced `{directive}` extensibility over Markdown.
  **CSVW/Frictionless**: schema side-loading confuses single-prompt LLM
  generation (schema and data in separate files).
- **Hard edges**: Excel caps floats at 15 significant digits (IEEE 754);
  locale separator conflict (`,` vs `;` argument separators) → serialize
  canonical en-US only; the value-vs-formula duality (drop cached values and
  every consumer needs a calc engine); circular references require the
  iterative-calc flag to be serialized; a custom EBNF/PEG grammar parsed
  99.99 % of 1M+ real formulas (Enron/EUSES corpora) — formal parsing is
  achievable, and Microsoft publishes no official grammar.
- **The report's recommendation** (which the spec follows): invent a hybrid —
  YAML frontmatter for workbook metadata; TOON-style tabular blocks for dense
  grids; SocialCalc-style per-cell directives for sparse cells; explicit
  value/formula duality; MDX-style escape hatch (raw OOXML/JSON fenced blocks)
  for exotic objects; strict en-US canonical locale.

### 3.2 DIO-0027 — the Excel-parity feature inventory

Paths: `/Users/lukerhodes/Downloads/spec-DIO-0027.md`,
`/Users/lukerhodes/Downloads/plan-DIO-0027.md`. A real web spreadsheet
editor's Excel-Online-parity spec (77+46 reference screenshots). It served as
the **feature checklist** the format must carry. Inventory (all must be
representable):

- **Formatting**: font family/size; bold/italic/underline **single+double**/
  strikethrough/sub/superscript; fill + font colors (theme/standard/recent/
  custom hex-RGB); borders (presets, per-edge, styles, colors, draw/erase);
  align h/v; orientation/rotation incl. vertical text; indent; wrap;
  **shrink-to-fit**; merge (& center / across / cells / unmerge).
- **Number formats**: all categories (General→Text incl. Fraction/Scientific/
  Special) + accounting currency locales + custom format codes with live
  sample.
- **Cell Styles gallery**: the full named catalog (Good/Bad/Neutral, Data &
  Model, Titles & Headings, Themed Accents at 20/40/60 %, Number Format).
- **Structural**: row/col insert/delete/hide/resize/auto-fit; sheet
  rename/reorder/duplicate/tab-color/hide; freeze/split/zoom/gridlines/
  headings; outline grouping + subtotals.
- **Conditional formatting**: highlight rules (incl. text-contains, date
  occurring, duplicates), top/bottom N & %, above/below average ± stddev,
  data bars (gradient/solid, negative axis), 2/3-color scales, icon sets
  (full catalog, custom thresholds, reverse, icons-only), formula rules;
  priority ordering + stop-if-true; manage-rules semantics.
- **Tables**: named ListObjects — header/total rows, banding, style gallery
  (Light/Medium/Dark), per-column filters, structured refs (`Tbl[Col]`,
  `[@col]`), auto-expand.
- **Sort/filter**: multi-level custom sort incl. by cell/font color and CF
  icon; AutoFilter value checklists + 11 condition operators + color filters.
- **Objects**: pictures (incl. place-in-cell), shapes gallery, text boxes,
  WordArt, checkboxes (form control **and** in-cell), hyperlinks + tooltips,
  notes (distinct from threaded comments), z-order.
- **Charts**: the full type set (column/bar/line/area/pie/doughnut/scatter/
  bubble/radar/stock/surface/histogram/pareto/box-whisker/treemap/sunburst/
  waterfall/funnel/map/combo, stacked/100 %/3-D variants); per-series
  fill/outline/gap/overlap; data labels; trendlines (linear/poly/exp/log/
  power/moving-avg + forecast + intercept + equation + R²); error bars; data
  table; axes (bounds/units/ticks/gridlines/titles/log/reverse/crosses/
  numfmt); legend; secondary axis; PivotCharts; **sparklines**.
- **Pivots**: rows/cols/values/filters, all agg functions, show-as modes,
  layout compact/outline/tabular, grand totals, sort-by-value.
- **Data/automation**: validation (all types + messages); Power-Query-like
  query pipelines (bounded transform set, not full M); macros as sandboxed JS
  scripts; **rich data types** (Stocks/Currencies/Geography with `=A1.Price`
  field access); slicers + timelines.
- **Formulas tab**: Name Manager (workbook + sheet scope), LAMBDA, calc modes
  (auto/manual + iterative), show-formulas view flag.
- **Protection**: sheet protect + allow-list, lock cell, hidden formula,
  workbook structure.
- **Page Layout**: margins/orientation/paper/scale-or-fit/print area/print
  titles/breaks/header-footer `&`-codes/gridlines-headings print.
- **Collaboration**: threaded comments with replies + resolved state.

### 3.3 XLSX domain knowledge assumed

1900/1904 date systems (and the 1900 leap-year bug at serial 60); shared
strings (dissolve in a text format); CSE `{=…}` vs dynamic-array spill;
external workbook references `[n]Sheet!Ref`; R1C1 (display-only, not storage);
theme = 12 color slots + major/minor fonts; dxf differential styles; EMU/
character-width/point unit systems; VBA as an opaque binary part; error-value
set incl. `#SPILL!`, `#CALC!`, `#FIELD!`, `#BLOCKED!`, `#GETTING_DATA`.

## 4. Load-bearing design decisions (attack these deliberately)

Each was a judgment call; the review should test them, not assume them.

1. **` :: ` cached-value separator** — split at the *last* ` :: ` outside the
   formula's double-quoted strings. Rationale: `::` never appears in Excel
   formula syntax outside string literals. Test: pathological formulas
   (nested quotes, `""` doubling, ` :: ` inside strings, sheet names with
   spaces/quotes).
2. **Leading `'` forces text** (Excel parity) + `"…"` quoting with `""`
   doubling. Test: cells that *start* with `'` legitimately; round-trip of
   `'0042`; interaction with YAML quoting in multiline `value:`.
3. **Merges via `@ range { merge: true }`** rather than a sheet-level merge
   list. Rationale: locality. Cost: merge state is scattered.
4. **CF priority = document order, with optional explicit `priority:`**. Test:
   interleaved-priority workbooks (rule priorities alternating across two
   ranges), and verify canonical formatting preserves effective order instead
   of sorting `{cf}` blocks by anchor.
5. **Row/col sizes in px** — converters own the character-width/point math
   (INTEROP §2). Test: lossless round-trip claims vs Excel's width formula.
6. **Dates as ISO 8601 text, typed by pattern** (scalar rule 8). Test: text
   cells that look like dates; the serial-60 bug; time-only values; date
   arithmetic parity across 1900/1904.
7. **Pivot cache records not serialized**; `refresh: on-load` is the contract;
   OLAP pivots ride `fallback:`. Cost: a GridMD file's pivot can show stale
   numbers only if the app cached rendered output elsewhere — is that
   acceptable for the product's use cases?
8. **Numbers as shortest round-trip IEEE-754 decimal** (ECMAScript
   `Number::toString`). Test against Excel's 15-digit display cap and
   financial-audit expectations.
9. **Relative fill** (`@ B2:B10 =A2*1.1` translates like Excel fill).
   Powerful for LLMs; test the interaction with absolute refs, structured
   refs, and cross-sheet refs.
10. **Formulas verbatim except locale canonicalization** — no whitespace or
    case normalization. Tension with §12 canonical form (byte-identical
    serialization of semantically identical workbooks): two authors writing
    `=SUM(A1 , A2)` vs `=SUM(A1,A2)` produce different canonical bytes. Is
    the canonical-form claim overstated?
11. **YAML safe subset** (no tags/aliases/anchors/multi-doc). Verify every
    example actually stays inside it, and that flow maps on `@` lines are
    parseable without a full YAML parser.
12. **Grid blank cells "do not clear"** + "each cell defined at most once"
    (strict) / last-write-wins (lenient). Test: is state-vs-edit semantics
    consistent everywhere (e.g. `{table}` over a range that `@` also touches)?

## 5. Known weak spots the author would flag first

Honest pre-disclosure — start here for cheap findings. Items marked "resolved
in-pass" were addressed in the 2026-07-04 spec cleanup; a **reference parser +
linter now exists** (`src/`, `bin/gridmd-lint.js`, 52 tests) and the example
lints clean in strict mode (5 sheets, 140 defined cells, 0 errors).

- The example is machine-validated for **grammar + semantic rules**, but
  cached values are still hand-computed, not engine-computed (e.g.
  `SUM(Sales[total]) :: 2003.05`, `AVERAGE(margin) :: 0.3771428571428571`,
  `C4*TaxRate :: 600.915`) — a calc engine or XLSX round-trip would pin them.
  First real finding from the linter: structured references inside YAML
  **flow** collections must be quoted (`[` is reserved there) — now SPEC §10.
- **`{cf}` cross-block priority** (see 4.4) — resolved-in-pass via optional
  explicit `priority:`, but test import/export of mixed explicit and implicit
  priorities.
- **Chart-sheet syntax** (`at sheet` + `{sheet} kind: chart`, DIRECTIVES §5) —
  resolved-in-pass at the grammar level; a worked example now exists
  (`examples/quarterly-report.gmd`, `# Revenue Chart`).
- **Spill cached values** — resolved-in-pass via `{spill-cache}`; validate that
  cache cells never count as independent cell definitions.
- **Table columns keyed by header name** — resolved-in-pass: headers must be
  non-empty text (no formulas), unique case-insensitively; importers
  disambiguate Excel-style (`2`, `3`, …). Enforced by the linter.
- **`@`-line props are "YAML flow mapping"** — resolved-in-pass with a
  right-edge split rule, but parser tests need formulas with array constants,
  structured refs, quoted commas, and trailing props.
- **Defined-name `value:` constants** — resolved-in-pass: SPEC §3 now carries
  a worked quoting example (array constant + embedded apostrophe).
- **Built-in style names** — resolved-in-pass: renamed to Excel's real names
  (`note`, `hyperlink`, `comma`, …) with an explicit key-vs-value namespace
  note and a user-shadowing rule (FORMATTING §5).
- **EBNF appendix** — resolved-in-pass: rewritten with the dedent rule, the
  right-edge props rule, quoted fence args, and fence-closing formalized
  (SPEC Appendix A); the reference parser implements it.
- **No `{find}`/UI-state directives** — deliberate (F3 class), but check the
  F3 list against DIO-0027 for anything that is actually persistent state
  (e.g. custom views, `Watch Window` lists).

## 6. Review protocol

Review in this order, reporting findings with severity:

1. **Internal consistency** — every syntax used in one doc is defined in
   another; the example validates; cross-references (§ numbers) resolve;
   canonical emission order (SPEC §12.2) matches the DIRECTIVES catalog order.
2. **Completeness vs §3.2 inventory** — walk the DIO-0027 list; for each item
   name the GridMD construct + fidelity class, or file a gap.
3. **Grammar robustness** — the §4 attack list; try to construct a legal Excel
   artifact the grammar cannot express or parses ambiguously.
4. **LLM ergonomics** — would a model reliably *generate* this? Look for
   places where correctness depends on invisible state (counted columns,
   positional grids vs inserted columns, required cached values).
5. **Round-trip soundness** — pick 5 nasty XLSX fixtures (spilled array w/
   cross-sheet refs; interleaved-priority CF; OLAP pivot; chart with
   picture-fill series; workbook with 1904 dates + external links) and trace
   them through the INTEROP map on paper.
6. **Security §INTEROP.5** — anything executable-by-default, any injection
   path (formula injection on import, YAML bombs, `data:` URIs, `{raw}`
   smuggling on re-emit to XLSX).

Severity rubric: **Critical** = data loss / ambiguity with two legal parses /
security; **High** = fidelity gap vs the §3.2 inventory with no escape-hatch
path; **Medium** = spec contradiction or underspecification an implementer
would hit; **Low** = naming, style, docs.

Output: a findings list (severity-ordered, each with the file/section, the
failing input or scenario, and a proposed fix), then an overall verdict on
whether 0.1 is ready for a reference-parser build.

## 7. Out of scope for the review

- Marketing naming (GridMD vs SheetMark vs Lattice — separately tracked).
- Whether to publish as an open standard (decided: product-internal first).
- Parser implementation choices (PEG vs hand-rolled) — but do flag grammar
  constructs that would make either materially harder.
