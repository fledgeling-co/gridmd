"""GridMD semantic validation (SPEC.md §9.4, §12-§13; DIRECTIVES.md).

A faithful port of ``js/src/validate.ts``: strict-mode structural rules
(define-once, table/spill-cache/name integrity, target validity) plus the
frontmatter and per-directive option checks. Errors reject a document; warnings
do not. User input is always turned into diagnostics, never a traceback.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any, Callable

from .parser import AtBlock, Diagnostic, FenceBlock, ParsedDocument, ParseStats, RESERVED_KINDS
from .refs import MAX_COL, MAX_ROW, Target, parse_cell, parse_target, ref_key
from .scalar import parse_scalar

_SHEET_NAME_BAD = re.compile(r"[:\\/?*\[\]]")
_TABLE_NAME_RE = re.compile(r"^[A-Za-z_\\][A-Za-z0-9_.\\]{0,254}$")
_CELLISH_NAME_RE = re.compile(r"^[A-Za-z]{1,3}\d+$")
_COLOR_RE = re.compile(r"^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$")
_THEME_COLOR_RE = re.compile(r"^(dk1|lt1|dk2|lt2|accent[1-6]|hlink|folHlink)(@-?\d{1,3})?$")

_WORKBOOK_KINDS = frozenset(["query", "script", "raw"])
_CONTENT_KEYS = ("value", "formula", "rich", "entity")
_FILL_ENUMERATION_CAP = 10000

_KNOWN_PROPS = frozenset(
    [
        "style", "font", "size", "bold", "italic", "underline", "strike", "sub",
        "super", "color", "fill", "pattern", "fill2", "border", "border-top",
        "border-right", "border-bottom", "border-left", "border-diag-up",
        "border-diag-down", "border-inner", "border-inner-h", "border-inner-v",
        "align", "valign", "rotation", "indent", "wrap", "shrink", "numfmt",
        "merge", "locked", "hidden", "link", "tip", "note", "rich", "spill",
        "array", "control", "entity", "fields", "value", "formula",
    ]
)
_SHEET_META_KEYS = frozenset(
    [
        "kind", "tab-color", "hidden", "freeze", "split", "view",
        "default-row-height", "default-col-width", "cols", "rows", "protect", "names",
    ]
)
_FRONTMATTER_KEYS = frozenset(
    [
        "gridmd", "title", "properties", "locale", "date-system", "calc", "theme",
        "names", "styles", "table-styles", "links", "protection",
    ]
)
_CHART_TYPES = frozenset(
    [
        "column", "bar", "line", "area", "pie", "doughnut", "scatter", "bubble",
        "radar", "stock", "surface", "histogram", "pareto", "box-whisker",
        "treemap", "sunburst", "waterfall", "funnel", "map", "combo",
    ]
)
_SHAPE_KINDS = frozenset(
    [
        "rect", "rounded-rect", "ellipse", "triangle", "right-triangle", "diamond",
        "pentagon", "hexagon", "star", "arrow-right", "arrow-left", "arrow-up",
        "arrow-down", "chevron", "callout", "line", "connector",
    ]
)
_VALIDATION_TYPES = frozenset(["list", "whole", "decimal", "date", "time", "text-length", "custom"])
_CF_RULE_KEYS = [
    "when", "contains", "not-contains", "begins", "ends", "date", "dupes",
    "unique", "top", "bottom", "avg", "bars", "scale", "icons", "formula",
]


def chart_base_type(t: str) -> str:
    base = t
    for suf in ("-stacked100", "-stacked", "-3d"):
        if base.endswith(suf):
            base = base[: -len(suf)]
    return base


def is_color(v: Any) -> bool:
    return isinstance(v, str) and (v == "auto" or bool(_COLOR_RE.match(v)) or bool(_THEME_COLOR_RE.match(v)))


def is_safe_link(v: Any) -> bool:
    return isinstance(v, str) and bool(re.match(r"^(https://|mailto:|#)", v))


def is_safe_image_src(v: Any) -> bool:
    if not isinstance(v, str):
        return False
    if re.match(r"^(javascript|vbscript|file):", v, re.I):
        return False
    if re.match(r"^data:", v, re.I):
        return bool(re.match(r"^data:image/", v, re.I))
    if re.match(r"^[a-z][a-z0-9+.-]*:", v, re.I):
        return bool(re.match(r"^https:", v, re.I))
    return True  # relative path


def is_valid_part_path(p: Any) -> bool:
    """``{raw}`` ``part=`` path rules (DIRECTIVES §18)."""
    if not isinstance(p, str) or p == "":
        return False
    if p.startswith("/") or "\\" in p:
        return False
    if re.search(r"[\x00-\x1f ]", p):
        return False
    if re.search(r"%2e|%2f|%5c", p, re.I):
        return False
    return all(s not in ("", ".", "..") for s in p.split("/"))


@dataclass
class _Ctx:
    target: Callable[[Any, int, list[str], str], Target | None]
    add_def: Callable[[int, int, int, str], None]


def _as_dict(v: Any) -> dict:
    return v if isinstance(v, dict) else {}


def _as_list(v: Any) -> list | None:
    return v if isinstance(v, list) else None


def validate_document(doc: ParsedDocument) -> ParsedDocument:
    errors = doc.errors
    warnings = doc.warnings

    def err(line: int, msg: str) -> None:
        errors.append(Diagnostic(line, msg))

    def warn(line: int, msg: str) -> None:
        warnings.append(Diagnostic(line, msg))

    stats = ParseStats()
    doc.stats = stats
    global_names: dict[str, str] = {}

    fm = _as_dict(doc.frontmatter)
    _validate_frontmatter(fm, err, warn, global_names)

    def validate_fence(b: FenceBlock, ctx: _Ctx | None = None) -> None:
        _validate_fence(b, ctx, err, warn, global_names)

    # ---- workbook-level blocks ----
    for b in doc.workbook_blocks:
        stats.blocks += 1
        if isinstance(b, AtBlock):
            err(b.line, "@ directives are not allowed before the first sheet")
            continue
        if b.kind.startswith("x-"):
            continue
        if b.kind not in RESERVED_KINDS:
            err(b.line, f"unknown directive {{{b.kind}}}")
            continue
        if b.kind not in _WORKBOOK_KINDS:
            err(b.line, f"{{{b.kind}}} is sheet-scoped and cannot appear before the first sheet")
            continue
        validate_fence(b)

    # ---- sheets ----
    if len(doc.sheets) == 0:
        err(1, "a workbook requires at least one sheet (a level-1 heading)")
    sheet_names: dict[str, Any] = {}
    for sheet in doc.sheets:
        name_key = sheet.name.lower()
        if len(sheet.name) > 31:
            err(sheet.line, f"sheet name exceeds 31 chars: {sheet.name}")
        if _SHEET_NAME_BAD.search(sheet.name):
            err(sheet.line, f"sheet name contains a forbidden character (: \\ / ? * [ ]): {sheet.name}")
        if name_key in sheet_names:
            err(sheet.line, f"duplicate sheet name: {sheet.name}")
        sheet_names[name_key] = sheet
        _validate_sheet(sheet, err, warn, global_names, stats)

    return doc


def _validate_frontmatter(
    fm: dict, err: Callable, warn: Callable, global_names: dict[str, str]
) -> None:
    gridmd = fm.get("gridmd")
    if not isinstance(gridmd, str) or not re.match(r"^\d+\.\d+$", gridmd):
        err(2, 'frontmatter requires gridmd: "MAJOR.MINOR" (quoted string)')
    for k in fm.keys():
        if k not in _FRONTMATTER_KEYS and not str(k).startswith("x-"):
            warn(2, f"unknown frontmatter key: {k}")
    if "date-system" in fm and fm["date-system"] not in (1900, 1904):
        err(2, "date-system must be 1900 or 1904")
    calc = _as_dict(fm.get("calc"))
    if "mode" in calc and calc["mode"] not in ("auto", "auto-no-tables", "manual"):
        err(2, f"calc.mode must be auto | auto-no-tables | manual, got {calc['mode']}")
    names = fm.get("names")
    for n in names if isinstance(names, list) else []:
        if not isinstance(n, dict) or not isinstance(n.get("name"), str):
            err(2, "names entries require a name")
            continue
        forms = [k for k in ("ref", "formula", "value") if n.get(k) is not None]
        if len(forms) != 1:
            err(2, f"name {n['name']}: exactly one of ref | formula | value required")
        if n["name"].lower() in global_names:
            err(2, f"duplicate defined name: {n['name']}")
        global_names[n["name"].lower()] = "name"
    for name, style in _as_dict(fm.get("styles")).items():
        if not isinstance(style, dict):
            err(2, f"style {name} must be a mapping")
    theme = _as_dict(fm.get("theme"))
    for slot, v in _as_dict(theme.get("colors")).items():
        if not re.match(r"^(dk1|lt1|dk2|lt2|accent[1-6]|hlink|folHlink)$", str(slot)):
            warn(2, f"unknown theme color slot: {slot}")
        elif not _COLOR_RE.match(str(v)):
            err(2, f"theme color {slot} must be #RRGGBB")


def _validate_fence(
    b: FenceBlock, ctx: _Ctx | None, err: Callable, warn: Callable, global_names: dict[str, str]
) -> None:
    meta = _as_dict(b.meta)
    pos = b.args.positional
    kind = b.kind

    def need(cond: Any, msg: str) -> None:
        if not cond:
            err(b.line, f"{{{kind}}} {msg}")

    if kind == "grid":
        anchor = parse_cell(pos[0] if pos else "")
        need(anchor, "requires a cell anchor")
        if not anchor:
            return
        for ri, row in enumerate(b.rows or []):
            for ci, cell_text in enumerate(row.cells):
                s = parse_scalar(cell_text)
                if s.problem:
                    err(row.line, f"grid cell: {s.problem}")
                if s.kind != "blank" and ctx:
                    ctx.add_def(anchor.col + ci, anchor.row + ri, row.line, "{grid}")
    elif kind == "table":
        _validate_table(b, meta, pos, ctx, err, need, global_names)
    elif kind == "cf":
        assert ctx is not None
        need(ctx.target(pos[0] if pos else None, b.line, ["cell", "range", "cols", "rows"], "{cf}"), "requires a target range")
        rules = _as_list(b.meta)
        need(rules is not None, "body must be a YAML list of rules")
        for rule in rules or []:
            rd = _as_dict(rule)
            kinds = [k for k in _CF_RULE_KEYS if rd.get(k) is not None]
            if len(kinds) != 1:
                err(b.line, "each cf rule needs exactly one distinguishing key")
            if rd.get("priority") is not None and (not isinstance(rd["priority"], int) or isinstance(rd["priority"], bool) or rd["priority"] < 1):
                err(b.line, "cf priority must be a positive integer")
            fmt = _as_dict(rd.get("format"))
            for key in ("fill", "color"):
                if fmt.get(key) is not None and not is_color(fmt[key]):
                    err(b.line, f"cf format.{key}: not a color: {fmt[key]}")
    elif kind == "validation":
        assert ctx is not None
        need(ctx.target(pos[0] if pos else None, b.line, ["cell", "range", "cols", "rows"], "{validation}"), "requires a target")
        need(meta.get("type") in _VALIDATION_TYPES, f"type must be one of {' | '.join(sorted(_VALIDATION_TYPES))}")
        if meta.get("type") == "list":
            need("values" in meta or "source" in meta, "list validation requires values: or source:")
        error = _as_dict(meta.get("error"))
        if error.get("style") is not None:
            need(error["style"] in ("stop", "warning", "information"), "error.style must be stop | warning | information")
    elif kind == "filter":
        assert ctx is not None
        need(ctx.target(pos[0] if pos else None, b.line, ["range"], "{filter}"), "requires a range")
        for k in _as_dict(meta.get("cols")).keys():
            if not re.match(r"^[A-Z]{1,3}$", str(k)):
                err(b.line, f"filter cols keys are column letters on plain ranges: {k}")
    elif kind == "chart":
        chart_type = pos[0] if pos else None
        if chart_type is not None and chart_base_type(chart_type) not in _CHART_TYPES:
            warn(b.line, f"unknown chart type {chart_type} — a converter must carry it via fallback:")
        need(b.args.anchor, "requires `at <anchor>` (or `at sheet` on a chart sheet)")
        if b.args.anchor and b.args.anchor != "sheet" and ctx:
            ctx.target(b.args.anchor, b.line, ["cell", "range"], "{chart} at")
        need("series" in meta or "data" in meta or "pivot" in meta, "requires series:, data:, or pivot:")
        series = meta.get("series")
        for i, s in enumerate(series if isinstance(series, list) else []):
            sd = _as_dict(s)
            if not s or (sd.get("val") is None and "pivot" not in meta):
                err(b.line, f"series[{i}] requires val:")
            if sd.get("color") is not None and not is_color(sd["color"]):
                err(b.line, f"series[{i}].color: not a color")
    elif kind == "sparklines":
        assert ctx is not None
        need(ctx.target(pos[0] if pos else None, b.line, ["cell", "range"], "{sparklines}"), "requires a target range")
        need("source" in meta, "requires source:")
        if meta.get("type") is not None:
            need(meta["type"] in ("line", "column", "win-loss"), "type must be line | column | win-loss")
    elif kind == "pivot":
        need(isinstance(pos[0] if pos else None, str), "requires a name")
        need(parse_cell(re.sub(r"^.*!", "", b.args.anchor or "")), "requires `at <cell>`")
        need("source" in meta, "requires source:")
        if pos and isinstance(pos[0], str):
            key = pos[0].lower()
            if key in global_names:
                err(b.line, f"pivot name collides with an existing name: {pos[0]}")
            global_names[key] = "pivot"
    elif kind == "slicer":
        need(b.args.anchor, "requires an anchor")
        need("for" in meta and "field" in meta, "requires for: and field:")
    elif kind == "image":
        need(b.args.anchor, "requires an anchor")
        need(isinstance(meta.get("src"), str), "requires src:")
        if isinstance(meta.get("src"), str) and not is_safe_image_src(meta["src"]):
            err(b.line, f"image src fails the scheme allowlist: {meta['src']}")
    elif kind == "shape":
        if pos and pos[0] not in _SHAPE_KINDS:
            warn(b.line, f"unknown shape kind {pos[0]} — carry exotic geometry via fallback:")
        need(b.args.anchor, "requires an anchor")
    elif kind == "textbox":
        need(b.args.anchor, "requires an anchor")
    elif kind == "checkbox":
        need(b.args.anchor, "requires an anchor")
        need("linked" not in meta or parse_cell(re.sub(r"\$", "", str(meta["linked"]))), "linked: must be a cell")
    elif kind == "comments":
        assert ctx is not None
        need(ctx.target(pos[0] if pos else None, b.line, ["cell"], "{comments}"), "requires a cell target")
        lst = _as_list(b.meta)
        need(lst is not None, "body must be a YAML list of comments")
        for c in lst or []:
            cd = _as_dict(c)
            if not cd.get("by") or not cd.get("at") or not cd.get("text"):
                err(b.line, "each comment requires by:, at:, text:")
    elif kind == "outline":
        for r in _as_list(meta.get("rows")) or []:
            if not re.match(r"^\d+:\d+$", str(_as_dict(r).get("range", ""))):
                err(b.line, f"outline rows range must be \"n:m\": {_as_dict(r).get('range')}")
        for c in _as_list(meta.get("cols")) or []:
            if not re.match(r"^[A-Z]{1,3}:[A-Z]{1,3}$", str(_as_dict(c).get("range", ""))):
                err(b.line, f"outline cols range must be \"A:B\": {_as_dict(c).get('range')}")
    elif kind == "page":
        if meta.get("orientation") is not None:
            need(meta["orientation"] in ("portrait", "landscape"), "orientation must be portrait | landscape")
        need(not ("scale" in meta and "fit" in meta), "scale: and fit: are mutually exclusive")
    elif kind == "query":
        need(isinstance(pos[0] if pos else None, str), "requires a name")
        need("source" in meta, "requires source:")
        need("steps" not in meta or isinstance(meta.get("steps"), list), "steps: must be a list")
    elif kind == "script":
        need(isinstance(pos[0] if pos else None, str), "requires a name")
        need(isinstance(b.args.flags.get("lang"), str), "requires lang=")
        need((b.code or "").strip() != "", "requires a code payload after ---")
    elif kind == "scenario":
        need(isinstance(pos[0] if pos else None, str), "requires a name")
        cells = meta.get("cells")
        need(isinstance(cells, dict), "requires cells:")
        for k in _as_dict(cells).keys():
            if not parse_cell(re.sub(r"\$", "", str(k))):
                err(b.line, f"scenario cells key must be a cell: {k}")
    elif kind == "raw":
        need((pos[0] if pos else "") in ("ooxml", "json", "text"), "format must be ooxml | json | text")
        if "part" in b.args.flags:
            need(is_valid_part_path(b.args.flags["part"]), f"part= fails package-path canonicalization: {b.args.flags['part']}")
        if "encoding" in b.args.flags:
            need(b.args.flags["encoding"] == "base64", "encoding must be base64")


def _validate_table(
    b: FenceBlock, meta: dict, pos: list, ctx: _Ctx | None, err: Callable, need: Callable, global_names: dict[str, str]
) -> None:
    name = pos[0] if pos else None
    need(
        isinstance(name, str) and bool(_TABLE_NAME_RE.match(name)) and not _CELLISH_NAME_RE.match(name),
        "requires a valid table name",
    )
    anchor = parse_cell(b.args.anchor or "")
    need(anchor, "requires `at <cell>`")
    if isinstance(name, str):
        key = name.lower()
        if key in global_names:
            err(b.line, f"table name collides with an existing name: {name}")
        global_names[key] = "table"
    rows = b.rows or []
    if not anchor or not rows:
        need(len(rows), "requires payload rows")
        return
    assert ctx is not None
    header = meta.get("header") is not False
    columns: list[str] = []
    for ri, row in enumerate(rows):
        for ci, cell_text in enumerate(row.cells):
            s = parse_scalar(cell_text)
            if s.problem:
                err(row.line, f"table cell: {s.problem}")
            if header and ri == 0:
                if s.kind != "text" or s.value == "":
                    err(row.line, f"table header cells must be non-empty text (column {ci + 1})")
                else:
                    columns.append(str(s.value))
                ctx.add_def(anchor.col + ci, anchor.row + ri, row.line, "{table} header")
                continue
            if s.kind != "blank":
                ctx.add_def(anchor.col + ci, anchor.row + ri, row.line, "{table}")
    lower = [c.lower() for c in columns]
    for i, c in enumerate(lower):
        if lower.index(c) != i:
            err(b.line, f"duplicate table column name: {columns[i]}")
    col_set = set(lower)

    def check_cols(obj: Any, what: str) -> None:
        for k in _as_dict(obj).keys():
            if str(k).lower() not in col_set:
                err(b.line, f"{what} references unknown column: {k}")

    check_cols(meta.get("cols"), "cols")
    check_cols(meta.get("total"), "total")
    check_cols(meta.get("filter"), "filter")
    for s in _as_list(meta.get("sort")) or []:
        if str(_as_dict(s).get("col", "")).lower() not in col_set:
            err(b.line, f"sort references unknown column: {_as_dict(s).get('col')}")
    total = meta.get("total")
    if isinstance(total, dict):
        total_row = anchor.row + len(rows)
        for col_name in total.keys():
            if str(col_name).lower() in lower:
                ci = lower.index(str(col_name).lower())
                ctx.add_def(anchor.col + ci, total_row, b.line, "{table} total")


def _validate_sheet(
    sheet: Any, err: Callable, warn: Callable, global_names: dict[str, str], stats: ParseStats
) -> None:
    defs: dict[str, int] = {}
    spills: list[Target] = []
    spill_caches: list[FenceBlock] = []
    sheet_metas: list[FenceBlock] = []
    charts_at_sheet = 0
    grid_content = 0

    def add_def(col: int, row: int, line: int, what: str) -> None:
        if col > MAX_COL or row > MAX_ROW:
            err(line, f"{what}: cell out of bounds")
            return
        k = ref_key(col, row)
        prev = defs.get(k)
        if prev is not None:
            err(line, f"{what}: cell defined more than once (previous definition at line {prev})")
            return
        defs[k] = line
        stats.defs += 1

    def target(text: Any, line: int, kinds: list[str], what: str) -> Target | None:
        t = parse_target(text or "")
        if not t or t.kind not in kinds:
            err(line, f"{what}: invalid target {text}")
            return None
        if t.sheet and t.sheet.lower() != sheet.name.lower():
            err(line, f"{what}: anchor qualifier {t.sheet}! must name the containing sheet ({sheet.name})")
        return t

    ctx = _Ctx(target=target, add_def=add_def)

    def validate_at(b: AtBlock) -> None:
        t = parse_target(b.target_text)
        if not t:
            err(b.line, f"invalid @ target: {b.target_text}")
            return
        if t.sheet and t.sheet.lower() != sheet.name.lower():
            err(b.line, f"@ target qualifier {t.sheet}! must name the containing sheet")
        body = _as_dict(b.body)
        props = {**_as_dict(b.props), **body}

        body_content_keys = [k for k in _CONTENT_KEYS if body.get(k) is not None]
        scalar = None
        if b.scalar_text is not None:
            scalar = parse_scalar(b.scalar_text)
            if scalar.problem:
                err(b.line, f"scalar: {scalar.problem}")
            if scalar.cached and scalar.cached.kind == "invalid":
                err(b.line, f"scalar: {scalar.cached.problem}")
            cached_only = len(body_content_keys) == 1 and body_content_keys[0] == "value" and scalar.kind == "formula"
            if body_content_keys and not cached_only:
                err(b.line, "inline content and body content keys on the same @ directive")
        has_formula = (scalar is not None and scalar.kind == "formula") or body.get("formula") is not None
        has_content = (scalar is not None and scalar.kind != "blank") or len(body_content_keys) > 0

        if has_content:
            if t.kind == "cell":
                add_def(t.c1, t.r1, b.line, "@")
            elif t.kind == "range" and has_formula:
                count = (t.r2 - t.r1 + 1) * (t.c2 - t.c1 + 1)
                if count > _FILL_ENUMERATION_CAP:
                    warn(b.line, f"relative fill over {count} cells — overlap checking skipped")
                else:
                    for r in range(t.r1, t.r2 + 1):
                        for c in range(t.c1, t.c2 + 1):
                            add_def(c, r, b.line, "@ fill")
            else:
                err(b.line, "range targets accept formula content only (relative fill, SPEC §8.5/§9.4)")

        for k, v in props.items():
            if k not in _KNOWN_PROPS and not str(k).startswith("x-"):
                warn(b.line, f"unknown property: {k}")
            if k in ("fill", "color") and not is_color(v):
                err(b.line, f"{k}: not a color: {v}")
            if k == "link" and not is_safe_link(v):
                err(b.line, f"link: scheme must be https:, mailto:, or internal #: {v}")
            if k == "merge":
                if t.kind != "range":
                    err(b.line, "merge: requires a range target")
                if v is not True:
                    err(b.line, "merge: only `true` is valid")
            if k in ("spill", "array"):
                st = parse_target(str(v))
                if not st or st.kind != "range":
                    err(b.line, f"{k}: must be a range")
                    continue
                if t.kind != "cell" or st.c1 != t.c1 or st.r1 != t.r1:
                    err(b.line, f"{k}: range must start at the anchor cell")
                st.line = b.line
                spills.append(st)
            if k == "rich" and not isinstance(v, list):
                err(b.line, "rich: must be a list of runs")
            if k == "control" and v != "checkbox":
                err(b.line, f"control: unknown control {v}")
        if body.get("formula") is not None and body.get("value") is None:
            warn(b.line, "formula without a cached value: readers will need a calc engine to display")

    def validate_sheet_meta(b: FenceBlock) -> None:
        m = _as_dict(b.meta)
        for k in m.keys():
            if k not in _SHEET_META_KEYS and not str(k).startswith("x-"):
                warn(b.line, f"unknown {{sheet}} key: {k}")
        if m.get("kind") is not None and m["kind"] not in ("worksheet", "chart"):
            err(b.line, "{sheet} kind must be worksheet | chart")
        if m.get("tab-color") is not None and not is_color(m["tab-color"]):
            err(b.line, f"tab-color: not a color: {m['tab-color']}")
        if m.get("hidden") is not None and m["hidden"] not in (True, False, "very"):
            err(b.line, "hidden must be false | true | very")
        for key in ("freeze", "split"):
            if m.get(key) is not None and not parse_cell(str(m[key])):
                err(b.line, f"{key}: must be a cell reference")
        for k, v in _as_dict(m.get("cols")).items():
            if not re.match(r"^[A-Z]{1,3}(:[A-Z]{1,3})?$", str(k)):
                err(b.line, f"cols key must be a column or column range: {k}")
            if not isinstance(v, (int, float)) and not isinstance(v, dict):
                err(b.line, f"cols.{k}: must be a width or a mapping")
        for k in _as_dict(m.get("rows")).keys():
            if not re.match(r"^\d+(:\d+)?$", str(k)):
                err(b.line, f"rows key must be a row or row range: {k}")

    for b in sheet.blocks:
        stats.blocks += 1
        if isinstance(b, AtBlock):
            validate_at(b)
            continue
        if b.kind.startswith("x-"):
            continue
        if b.kind not in RESERVED_KINDS:
            err(b.line, f"unknown directive {{{b.kind}}}")
            continue
        if b.kind == "sheet":
            sheet_metas.append(b)
            validate_sheet_meta(b)
            continue
        if b.kind in ("grid", "table"):
            grid_content += 1
        if b.kind == "spill-cache":
            spill_caches.append(b)
            continue
        if b.kind == "chart" and b.args.anchor == "sheet":
            charts_at_sheet += 1
        _validate_fence(b, ctx, err, warn, global_names)

    if len(sheet_metas) > 1:
        err(sheet_metas[1].line, "multiple {sheet} blocks in one sheet")
    if sheet_metas and sheet.blocks[0] is not sheet_metas[0]:
        warn(sheet_metas[0].line, "{sheet} should be the first block of its sheet")
    meta = _as_dict(sheet_metas[0].meta) if sheet_metas else {}

    if meta.get("kind") == "chart":
        if charts_at_sheet != 1:
            err(sheet.line, f"a chart sheet requires exactly one {{chart}} anchored `at sheet` (found {charts_at_sheet})")
        if grid_content > 0 or len(defs) > 0:
            err(sheet.line, "a chart sheet cannot carry worksheet grid content")
    elif charts_at_sheet > 0:
        err(sheet.line, "`at sheet` chart anchors require {sheet} kind: chart")

    for sc in spill_caches:
        anchor = parse_cell(sc.args.positional[0] if sc.args.positional else "")
        if not anchor:
            err(sc.line, "{spill-cache} requires a cell anchor")
            continue
        rows = sc.rows or []
        h = len(rows)
        w = max([0] + [len(r.cells) for r in rows])
        owner = next((s for s in spills if s.c1 == anchor.col and s.r1 == anchor.row), None)
        if not owner:
            err(sc.line, f"{{spill-cache}} at {sc.args.positional[0] if sc.args.positional else ''} has no owning spill/array formula at that anchor")
            continue
        if anchor.row + h - 1 > owner.r2 or anchor.col + w - 1 > owner.c2:
            err(sc.line, "{spill-cache} rectangle exceeds the declared spill/array range")
