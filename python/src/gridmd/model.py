"""Block tree → per-sheet workbook model (the Tier-1 slice the dump + XLSX
worksheet core consume): effective cells, merges, tables, and the feature-block
lists the canonical dump counts.

Styling/patches are intentionally not materialized — the dump only reports
content-bearing cells and the Tier-1 XLSX worksheet core emits values, not
formatting (SPEC §11: everything else round-trips through the source carry part).
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any

from .parser import AtBlock, FenceBlock, ParsedDocument
from .refs import CellPos, Target, col_to_num, num_to_col, parse_cell, parse_target, ref_key
from .scalar import Scalar, parse_scalar

_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?)?$")
_TIME_RE = re.compile(r"^\d{2}:\d{2}(:\d{2})?$")

_CONTENT_KEYS = ("value", "formula", "rich", "entity", "fields", "spill", "array")


@dataclass
class CellContent:
    formula: str | None = None
    cse: bool = False
    cached: Scalar | None = None
    scalar: Scalar | None = None
    rich: Any = None
    array_ref: str | None = None
    entity_fields: Any = None
    spill_cache: bool = False
    has_cached: bool = False  # whether `cached` was explicitly supplied


@dataclass
class Cell:
    col: int
    row: int
    content: CellContent | None = None


@dataclass
class TableModel:
    name: str
    anchor: CellPos
    columns: list[str]
    header_row: bool
    body_rows: int
    total: Any
    line: int


@dataclass
class Sheet:
    name: str
    meta: dict
    kind: str
    cells: dict[str, Cell] = field(default_factory=dict)
    merges: list[Target] = field(default_factory=list)
    tables: list[TableModel] = field(default_factory=list)
    cf: list[Any] = field(default_factory=list)
    validations: list[Any] = field(default_factory=list)
    notes: list[Any] = field(default_factory=list)
    threads: list[Any] = field(default_factory=list)
    scenarios: list[Any] = field(default_factory=list)
    sparklines: list[Any] = field(default_factory=list)
    charts: list[Any] = field(default_factory=list)
    pivots: list[Any] = field(default_factory=list)
    slicers: list[Any] = field(default_factory=list)
    images: list[Any] = field(default_factory=list)
    shapes: list[Any] = field(default_factory=list)
    hyperlinks: list[Any] = field(default_factory=list)


@dataclass
class WorkbookModel:
    fm: dict
    sheets: list[Sheet]


def _coalesce(a: Any, b: Any) -> Any:
    return a if a is not None else b


def _yaml_scalar(v: Any) -> Scalar:
    if isinstance(v, bool):
        return Scalar(kind="boolean", value=v)
    if isinstance(v, (int, float)):
        return Scalar(kind="number", value=float(v))
    s = str(v)
    if _DATE_RE.match(s):
        return Scalar(kind="date", value=s)
    if _TIME_RE.match(s):
        return Scalar(kind="time", value=s)
    return Scalar(kind="text", value=s)


def build_workbook_model(doc: ParsedDocument) -> WorkbookModel:
    fm = doc.frontmatter if isinstance(doc.frontmatter, dict) else {}
    sheets: list[Sheet] = []

    for sheet_block in doc.sheets:
        meta = _sheet_meta(sheet_block)
        s = Sheet(name=sheet_block.name, meta=meta, kind="chart" if meta.get("kind") == "chart" else "worksheet")
        sheets.append(s)
        _materialize_sheet(sheet_block, s)

    return WorkbookModel(fm=fm, sheets=sheets)


def _sheet_meta(sheet_block: Any) -> dict:
    for b in sheet_block.blocks:
        if isinstance(b, FenceBlock) and b.kind == "sheet":
            return b.meta if isinstance(b.meta, dict) else {}
    return {}


def _materialize_sheet(sheet_block: Any, s: Sheet) -> None:
    def cell_at(col: int, row: int) -> Cell:
        k = ref_key(col, row)
        c = s.cells.get(k)
        if c is None:
            c = Cell(col=col, row=row)
            s.cells[k] = c
        return c

    def set_content(col: int, row: int, content: CellContent) -> None:
        c = cell_at(col, row)
        if c.content is None:
            c.content = content
        elif content.has_cached and c.content.formula is not None and c.content.cached is None:
            c.content.cached = content.cached

    def scalar_content(sc: Scalar) -> CellContent:
        if sc.kind == "formula":
            return CellContent(formula=sc.formula, cse=sc.cse, cached=sc.cached, has_cached=True)
        return CellContent(scalar=sc)

    def body_content(body: dict, flow: dict) -> CellContent | None:
        if "formula" in body:
            f = re.sub(r"^=", "", str(body["formula"]))
            has_val = "value" in body
            content = CellContent(
                formula=f, cse=False,
                cached=_yaml_scalar(body["value"]) if has_val else None, has_cached=True,
            )
            spill = _coalesce(body.get("spill"), flow.get("spill"))
            arr = _coalesce(body.get("array"), flow.get("array"))
            if spill or arr:
                content.array_ref = str(_coalesce(spill, arr))
            if arr:
                content.cse = True
            return content
        if "rich" in body:
            return CellContent(rich=body["rich"])
        if "entity" in body:
            ent = body["entity"] if isinstance(body["entity"], dict) else {}
            text = _coalesce(_coalesce(ent.get("text"), ent.get("id")), "")
            return CellContent(scalar=Scalar(kind="text", value=str(text)), entity_fields=body.get("fields", {}))
        if "value" in body:
            return CellContent(scalar=_yaml_scalar(body["value"]))
        return None

    def apply_at(b: AtBlock) -> None:
        t = parse_target(b.target_text)
        if not t:
            return
        body = b.body if isinstance(b.body, dict) else {}
        flow = b.props if isinstance(b.props, dict) else {}

        if b.scalar_text is not None:
            sc = parse_scalar(b.scalar_text)
            if t.kind == "cell" and sc.kind != "blank":
                content = scalar_content(sc)
                if content.formula is not None:
                    spill = _coalesce(flow.get("spill"), body.get("spill"))
                    arr = _coalesce(flow.get("array"), body.get("array"))
                    if spill or arr:
                        content.array_ref = str(_coalesce(spill, arr))
                    if arr:
                        content.cse = True
                set_content(t.c1, t.r1, content)
            elif t.kind == "range" and sc.kind == "formula":
                for r in range(t.r1, t.r2 + 1):
                    for c in range(t.c1, t.c2 + 1):
                        set_content(
                            c, r,
                            CellContent(formula=translate_formula(sc.formula or "", r - t.r1, c - t.c1),
                                        cse=False, cached=None, has_cached=True),
                        )
        else:
            content = body_content(body, flow)
            if content and t.kind == "cell":
                set_content(t.c1, t.r1, content)

        props = {**flow, **{k: v for k, v in body.items() if k not in _CONTENT_KEYS}}
        merge = props.get("merge")
        if merge is True and t.kind == "range":
            s.merges.append(t)
        link = props.get("link")
        if link:
            s.hyperlinks.append({"col": t.c1, "row": t.r1, "target": link, "tip": props.get("tip")})
        note = _coalesce(props.get("note"), body.get("note"))
        if note is not None:
            s.notes.append({"col": t.c1, "row": t.r1, "text": str(note)})

    def apply_fence(b: FenceBlock) -> None:
        kind = b.kind
        meta = b.meta if b.meta is not None else {}
        pos = b.args.positional
        if kind == "sheet":
            return
        if kind == "grid":
            a = parse_cell(pos[0]) if pos else None
            if a is None:
                return
            for ri, row in enumerate(b.rows or []):
                for ci, text in enumerate(row.cells):
                    sc = parse_scalar(text)
                    if sc.kind != "blank":
                        set_content(a.col + ci, a.row + ri, scalar_content(sc))
        elif kind == "spill-cache":
            a = parse_cell(pos[0]) if pos else None
            if a is None:
                return
            for ri, row in enumerate(b.rows or []):
                for ci, text in enumerate(row.cells):
                    sc = parse_scalar(text)
                    if sc.kind == "blank":
                        continue
                    if ri == 0 and ci == 0:
                        set_content(a.col, a.row, CellContent(cached=sc, has_cached=True))
                    else:
                        c = scalar_content(sc)
                        c.spill_cache = True
                        set_content(a.col + ci, a.row + ri, c)
        elif kind == "table":
            _apply_table(b, s, set_content)
        elif kind == "cf":
            s.cf.append(meta)
        elif kind == "validation":
            s.validations.append(meta)
        elif kind == "chart":
            s.charts.append({"anchor": b.args.anchor, "meta": meta})
        elif kind == "sparklines":
            s.sparklines.append(meta)
        elif kind == "pivot":
            s.pivots.append({"name": pos[0] if pos else None, "anchor": b.args.anchor})
        elif kind == "slicer":
            s.slicers.append({"anchor": b.args.anchor, "meta": meta})
        elif kind == "image":
            s.images.append({"anchor": b.args.anchor, "src": str(meta.get("src", ""))})
        elif kind == "shape":
            s.shapes.append({"preset": pos[0] if pos else "rect", "anchor": b.args.anchor})
        elif kind == "textbox":
            s.shapes.append({"preset": "textbox", "anchor": b.args.anchor})
        elif kind == "comments":
            s.threads.append({"ref": pos[0] if pos else None, "comments": meta})
        elif kind == "scenario":
            s.scenarios.append({"name": pos[0] if pos else None})
        # filter / outline / page / checkbox / raw / query / script / x-*:
        # not surfaced in the canonical dump counts.

    for b in sheet_block.blocks:
        if isinstance(b, AtBlock):
            apply_at(b)
        else:
            apply_fence(b)


def _apply_table(b: FenceBlock, s: Sheet, set_content: Any) -> None:
    a = parse_cell(b.args.anchor or "")
    if a is None:
        return
    tm = b.meta if isinstance(b.meta, dict) else {}
    header = tm.get("header") is not False
    columns: list[str] = []
    rows = b.rows or []
    for ri, row in enumerate(rows):
        for ci, text in enumerate(row.cells):
            sc = parse_scalar(text)
            if header and ri == 0 and sc.kind == "text":
                columns.append(str(sc.value))
            if sc.kind != "blank":
                set_content(a.col + ci, a.row + ri, _scalar_or_formula(sc))
    total = tm.get("total")
    if total:
        total_row = a.row + len(rows)
        for col_name, v in total.items():
            ci = next((i for i, c in enumerate(columns) if c.lower() == str(col_name).lower()), -1)
            if ci == -1:
                continue
            set_content(a.col + ci, total_row, _scalar_or_formula(parse_scalar(str(v))))
    s.tables.append(
        TableModel(
            name=b.args.positional[0] if b.args.positional else "",
            anchor=a, columns=columns, header_row=header,
            body_rows=len(rows) - (1 if header else 0), total=total, line=b.line,
        )
    )


def _scalar_or_formula(sc: Scalar) -> CellContent:
    if sc.kind == "formula":
        return CellContent(formula=sc.formula, cse=sc.cse, cached=sc.cached, has_cached=True)
    return CellContent(scalar=sc)


def translate_formula(formula: str, dr: int, dc: int) -> str:
    """Relative fill (SPEC §8.5): shift unanchored A1 refs by ``(dr, dc)``,
    skipping string literals and quoted sheet names."""
    ref_re = re.compile(r"^(\$?)([A-Z]{1,3})(\$?)(\d{1,7})(?![A-Za-z0-9_(])")
    out = ""
    i = 0
    n = len(formula)
    while i < n:
        ch = formula[i]
        if ch in ('"', "'"):
            q = ch
            j = i + 1
            while j < n:
                if formula[j] == q:
                    if j + 1 < n and formula[j + 1] == q:
                        j += 2
                        continue
                    break
                j += 1
            out += formula[i : j + 1]
            i = j + 1
            continue
        m = ref_re.match(formula[i:])
        prev = out[-1:] if out else ""
        if m and not re.match(r"[A-Za-z0-9_.]", prev):
            cd, col_l, rd, row_s = m.group(1), m.group(2), m.group(3), m.group(4)
            col = col_l if cd == "$" else num_to_col(max(1, col_to_num(col_l) + dc))
            row = row_s if rd == "$" else str(max(1, int(row_s) + dr))
            out += f"{cd}{col}{rd}{row}"
            i += len(m.group(0))
            continue
        out += ch
        i += 1
    return out
