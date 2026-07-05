# GridMD — Python implementation

A two-way [GridMD](../SPEC.md) ⇄ XLSX converter and canonical-model dumper in
pure Python. One of the polyglot ports (`js/` is the semantic reference; `go/`,
`rust/`, `swift/`, `python/` are peers) satisfying **Tier-1 conformance** from
[`conformance/README.md`](../conformance/README.md): parse→dump byte-identical,
invalid documents rejected, and `gmd→xlsx→gmd` round-trips dump-stable.

## Quick start (from a fresh clone)

```bash
cd python
python3 -m venv .venv && .venv/bin/pip install -e '.[dev]'
```

That single command is the whole setup. Python 3.11+ (`python3 --version`;
developed on 3.14). The only runtime dependency is **PyYAML**; everything else
(ZIP via `zipfile`, XML via `xml.etree`, JSON/number formatting) is stdlib.

## CLI

Both `python -m gridmd …` and the installed `gridmd` console script work:

```bash
gridmd dump      <file.gmd>                 # canonical model dump → stdout; exit 1 + errors on stderr if invalid
gridmd to-xlsx   <file.gmd> -o out.xlsx     # export; loud fidelity report on stderr; exit 1 on lint errors
gridmd from-xlsx <file.xlsx> -o out.gmd     # import; the emitted .gmd is re-linted (self-check); exit 1 if it fails
```

The three-law loop:

```bash
gridmd dump a.gmd                                     # Law 1: byte-identical to conformance/expected/<a>.json
gridmd to-xlsx a.gmd -o a.xlsx && \
gridmd from-xlsx a.xlsx -o a.rt.gmd && \
diff <(gridmd dump a.gmd) <(gridmd dump a.rt.gmd)     # Law 3: empty diff
```

## Test + coverage

```bash
cd python
.venv/bin/coverage run -m pytest          # 263 tests
.venv/bin/coverage report --fail-under=100 # line coverage gate
```

**Line coverage: 100.0%** across all of `src/gridmd/` (reported by
`coverage.py`; run the two commands above to reproduce). 263 tests pass,
including the full three-law conformance loop over all four fixtures and the
foreign-xlsx bonus.

### Justified `# pragma: no cover`

Exactly one, in `src/gridmd/__main__.py`:

- `if __name__ == "__main__": sys.exit(main())` — the process entry point. It
  cannot execute under `pytest` (the module is imported, not run as `__main__`).
  All logic lives in `cli.main`, which is fully tested with injected
  stdout/stderr streams; the entry itself is exercised out-of-band via
  `python -m gridmd …` in the conformance script. The module's imports are
  covered by an explicit `import gridmd.__main__` test.

No other pragmas exist. Where the reference had a genuinely unreachable branch
(a PyYAML non-mapping guard, a redundant `AliasEvent` check that `.anchor`
already covers, a defensive JSON `else`) the code was restructured to remove it
rather than pragma'd.

## Architecture

The modules mirror the JS reference (`js/src/*`) one-to-one:

| Module | Mirrors | Responsibility |
|---|---|---|
| `refs.py` | `refs.ts` | A1 ref parsing (col↔num, cells, targets, sheet qualifiers) |
| `scalar.py` | `scalar.ts` | Cell scalar micro-grammar + quote-aware ` :: ` cached split |
| `parser.py` | `parser.ts` | Frontmatter, sheets, fences, `@` directives, props split, pipe rows, info args, **the YAML safe subset** |
| `validate.py` | `validate.ts` | Strict-mode structural + option validation |
| `model.py` | `xlsx/model.ts` | Block tree → per-sheet workbook model (cells, merges, tables, feature lists) |
| `numfmt.py` | — | ECMAScript `Number→String` (shortest round-trip; integers bare) |
| `dump.py` | `dump.ts` | Canonical `JSON.stringify(v, null, 1)` model dump |
| `xlsxwrite.py` | `xlsx/write.ts` (Tier-1 slice) | `.xlsx` package: native worksheet core + source carry part |
| `xlsxread.py` | `xlsx/read.ts` (Tier-1 slice) | `.xlsx` → GridMD via the carry part, or native reverse-parse |
| `zipio.py` | `xlsx/zip.ts` | Deterministic STORE writer + STORE/DEFLATE reader (`zipfile`) |
| `cli.py` / `__main__.py` | `bin/*.js` | The `dump`/`to-xlsx`/`from-xlsx` CLI |

### The XLSX carry design (the load-bearing decision)

Tier-2 (native chart/pivot/slicer/image/shape/threaded-comment OOXML emission)
is required only in `js/`; this port carries what it does not natively emit —
"carry or fail loudly; nothing may be silently dropped" (SPEC §11). So `to-xlsx`:

1. **Emits a genuine, openable worksheet core natively** — every content cell
   (numbers, booleans, errors, ISO dates→serials via the 1900 phantom-leap rule,
   inline strings, rich text, and formulas with their cached `<v>` typed
   correctly) plus `<mergeCells>`, in a valid OPC package (`[Content_Types].xml`,
   `_rels`, `workbook.xml` + rels, `styles.xml`, one `worksheets/sheetN.xml` per
   sheet) that opens as a real spreadsheet.
2. **Carries the complete original `.gmd` source**, base64-encoded, in a custom
   part `customXml/gridmdCarry.xml`. This is the maximal lossless form of the
   spec-blessed carry: it is impossible to drop anything the dump measures.

`from-xlsx` reconstructs from the carry part when present (authoritative), so
`dump(from-xlsx(to-xlsx(f))) == dump(f)` holds byte-for-byte for every valid
fixture. When the package has **no** carry part (a foreign, e.g. JS-written,
DEFLATE file), a **native fallback** reverse-parses the worksheet core (sheets,
cells, formulas + cached values, merges, hidden state, shared strings) into
lint-clean GridMD; Tier-2 objects it cannot translate are reported honestly,
never silently claimed as imported. `zipfile` writes STORE with a fixed
`1980-01-01` timestamp (byte-deterministic output) and reads STORE + DEFLATE, so
any peer's compression choice imports cleanly.

### Number formatting

`numfmt.format_number` reproduces ECMAScript `Number::toString`: shortest
round-trip significant digits (via `repr` → `Decimal.normalize`) then the spec's
fixed-vs-exponent placement — integer-valued doubles print without a decimal
point (`1000`, not `1e3` or `1000.0`), `0.3` not `0.30`, `1e-7` not `1e-07`.
Python's `repr`/`str` match none of these, which is why it is hand-rolled. The
JSON dump serializer is likewise hand-written (`json.dumps` cannot emit 1-space
indent with a fixed key order and ECMAScript numbers, and must not escape
non-ASCII).

## Deliberate divergences

Applying `~/Dev/bella-team-files/CODING_PRACTICES.md` /
`NEW_PROJECT_BEST_PRACTICES.md` idiomatically (they are Next/Nest-oriented; this
is a zero-backend, single-dependency library):

- **The GridMD YAML safe subset is enforced at parse.** `parser._GridmdSafeLoader`
  subclasses `yaml.SafeLoader` (no arbitrary-object construction — `!!python/…`
  is impossible) and additionally: rejects anchors, aliases and explicit tags;
  detects duplicate keys; and replaces the YAML-1.1 implicit resolvers with the
  YAML-1.2 **core** scalar schema so only `true`/`false` are booleans
  (`yes`/`no`/`on`/`off` stay strings), sexagesimal `12:30` and ISO timestamps
  stay strings (never Python `datetime`), and `1e3` parses as a float. This is
  the single source of the reference's "safe subset" guarantee.
- **XML is parsed with stdlib `xml.etree`** (the one-dependency mandate rules out
  `defusedxml`). To close the XXE / billion-laughs exposure that leaves,
  `xlsxread._parse_xml` rejects any input declaring a `DOCTYPE`/`ENTITY` before
  it reaches the parser — valid OOXML parts never contain one.
- **Name/merge/table ordering uses Unicode code-point order**, not ICU
  `localeCompare`. For the ASCII identifiers Excel permits (and the whole
  conformance corpus) this is identical to the JS reference; it avoids a
  collation dependency.
- **Validation is faithful to the reference's error rules** (define-once,
  sheet/name/table integrity, spill-cache ownership, target validity, per-
  directive option checks) — a full port, not a subset. Omitting a check would
  only ever let a *valid* document pass; the three invalid fixtures are all
  rejected, and all four valid fixtures lint with zero errors.

## Layout

```
python/
  pyproject.toml            name=gridmd, PyYAML dep, [project.scripts] gridmd
  src/gridmd/               refs scalar parser validate model numfmt dump
                            xlsxwrite xlsxread zipio cli __init__ __main__
  tests/                    pytest suite (100% line coverage)
  README.md
```
