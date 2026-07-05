"""Canonical model dump — the cross-language conformance contract.

Byte-identical JSON to the reference ``dumpModel`` (``js/src/dump.ts``):
``JSON.stringify(value, null, 1)`` — 1-space indent, fixed key order (insertion
order, never alphabetized except the contract-sorted arrays), shortest
round-trip numbers, and a trailing newline.
"""

from __future__ import annotations

import math
from typing import Any

from .model import CellContent, Sheet, WorkbookModel
from .numfmt import format_number
from .refs import num_to_col
from .scalar import Scalar


def _scalar_dump(s: Scalar | None) -> Any:
    if s is None:
        return None
    if s.kind == "number":
        return {"t": "n", "v": s.value}
    if s.kind == "boolean":
        return {"t": "b", "v": s.value}
    if s.kind == "error":
        return {"t": "e", "v": s.value}
    if s.kind in ("date", "time"):
        return {"t": "d", "v": s.value}
    return {"t": "s", "v": "" if s.value is None else str(s.value)}


def _content_dump(ct: CellContent) -> Any:
    if ct.rich:
        text = "".join("" if r.get("text") is None else str(r.get("text")) for r in ct.rich)
        return {"t": "rich", "v": text}
    if ct.formula is not None:
        return {"t": "f", "f": ct.formula, "cached": _scalar_dump(ct.cached), "array": ct.array_ref}
    return _scalar_dump(ct.scalar)


def _js_string(v: Any) -> str:
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return format_number(v)
    return str(v)


def _dump_names(fm: dict) -> list:
    raw = fm.get("names") or []
    entries = []
    for item in raw:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        name = name if isinstance(name, str) else ""
        entry = {
            "name": name,
            "ref": item.get("ref") if item.get("ref") is not None else None,
            "formula": item.get("formula") if item.get("formula") is not None else None,
            "value": _js_string(item["value"]) if "value" in item else None,
        }
        entries.append((name, entry))
    entries.sort(key=lambda e: e[0])
    return [e[1] for e in entries]


def _dump_cells(s: Sheet) -> dict:
    content_cells = [c for c in s.cells.values() if c.content is not None]
    content_cells.sort(key=lambda c: (c.row, c.col))
    out: dict = {}
    for c in content_cells:
        out[f"{num_to_col(c.col)}{c.row}"] = _content_dump(c.content)
    return out


def _dump_merges(s: Sheet) -> list:
    refs = [f"{num_to_col(m.c1)}{m.r1}:{num_to_col(m.c2)}{m.r2}" for m in s.merges]
    refs.sort()
    return refs


def _dump_tables(s: Sheet) -> list:
    tables = sorted(s.tables, key=lambda t: t.name)
    out = []
    for t in tables:
        out.append(
            {
                "name": t.name,
                "anchor": f"{num_to_col(t.anchor.col)}{t.anchor.row}",
                "columns": list(t.columns),
                "bodyRows": t.body_rows,
                "hasTotals": bool(t.total),
            }
        )
    return out


def _cf_count(s: Sheet) -> int:
    return sum(len(rules) if isinstance(rules, list) else 0 for rules in s.cf)


def _dump_sheet(s: Sheet) -> dict:
    meta = s.meta
    hidden_raw = meta.get("hidden")
    hidden: Any = True if hidden_raw is True else "very" if hidden_raw == "very" else False
    freeze = meta.get("freeze")
    freeze = freeze if isinstance(freeze, str) else None
    protect = meta.get("protect")
    protected = bool(protect.get("enabled")) if isinstance(protect, dict) else False
    return {
        "name": s.name,
        "kind": s.kind,
        "hidden": hidden,
        "freeze": freeze,
        "protected": protected,
        "cells": _dump_cells(s),
        "merges": _dump_merges(s),
        "tables": _dump_tables(s),
        "counts": {
            "cf": _cf_count(s),
            "validations": len(s.validations),
            "notes": len(s.notes),
            "threads": len(s.threads),
            "scenarios": len(s.scenarios),
            "sparklines": len(s.sparklines),
            "charts": len(s.charts),
            "pivots": len(s.pivots),
            "slicers": len(s.slicers),
            "images": len(s.images),
            "shapes": len(s.shapes),
            "hyperlinks": len(s.hyperlinks),
        },
    }


def dump_model(model: WorkbookModel) -> str:
    fm = model.fm
    date_system = 1904 if fm.get("date-system") == 1904 else 1900
    out = {
        "gridmd": fm.get("gridmd") if _is_primitive(fm.get("gridmd")) else None,
        "title": fm.get("title") if _is_primitive(fm.get("title")) else None,
        "dateSystem": date_system,
        "names": _dump_names(fm),
        "sheets": [_dump_sheet(s) for s in model.sheets],
    }
    buf: list[str] = []
    _serialize(buf, out, 0)
    buf.append("\n")
    return "".join(buf)


def _is_primitive(v: Any) -> bool:
    return v is None or isinstance(v, (str, bool, int, float))


# ---- JSON serialization (JSON.stringify(v, null, 1)) ----
_ESCAPES = {'"': '\\"', "\\": "\\\\", "\n": "\\n", "\r": "\\r", "\t": "\\t", "\b": "\\b", "\f": "\\f"}


def _write_json_string(buf: list[str], s: str) -> None:
    buf.append('"')
    for ch in s:
        esc = _ESCAPES.get(ch)
        if esc is not None:
            buf.append(esc)
        elif ord(ch) < 0x20:
            buf.append(f"\\u{ord(ch):04x}")
        else:
            buf.append(ch)
    buf.append('"')


def _serialize(buf: list[str], v: Any, depth: int) -> None:
    if v is None:
        buf.append("null")
    elif isinstance(v, bool):
        buf.append("true" if v else "false")
    elif isinstance(v, str):
        _write_json_string(buf, v)
    elif isinstance(v, (int, float)):
        buf.append(format_number(v) if math.isfinite(float(v)) else "null")
    elif isinstance(v, dict):
        if not v:
            buf.append("{}")
            return
        buf.append("{\n")
        pad = " " * (depth + 1)
        for i, (k, val) in enumerate(v.items()):
            if i > 0:
                buf.append(",\n")
            buf.append(pad)
            _write_json_string(buf, k)
            buf.append(": ")
            _serialize(buf, val, depth + 1)
        buf.append("\n" + " " * depth + "}")
    elif isinstance(v, list):
        if not v:
            buf.append("[]")
            return
        buf.append("[\n")
        pad = " " * (depth + 1)
        for i, item in enumerate(v):
            if i > 0:
                buf.append(",\n")
            buf.append(pad)
            _serialize(buf, item, depth + 1)
        buf.append("\n" + " " * depth + "]")
    else:
        raise TypeError(f"cannot serialize {type(v).__name__} to canonical JSON")
