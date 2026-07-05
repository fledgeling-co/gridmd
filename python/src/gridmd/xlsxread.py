"""XLSX → GridMD.

If the package carries the original GridMD source (``customXml/gridmdCarry.xml``,
written by :mod:`gridmd.xlsxwrite`) the import restores it verbatim — a byte-exact
round-trip. Otherwise a native fallback reverse-parses the worksheet core (sheets,
cells, formulas + cached values, merges, hidden state) into lint-clean GridMD.
Foreign objects with no worksheet-core representation (charts, pivots, notes, …)
are reported, never silently claimed as imported.
"""

from __future__ import annotations

import base64
import re
import xml.etree.ElementTree as ET
from dataclasses import dataclass

from .refs import num_to_col, parse_cell
from .xlsxwrite import CARRY_PART
from .zipio import zip_read


@dataclass
class ImportReport:
    feature: str
    action: str
    note: str | None = None


def xlsx_to_gridmd(data: bytes) -> tuple[str, list[ImportReport]]:
    """Import an ``.xlsx`` buffer into GridMD source + a fidelity report."""
    entries = zip_read(data)
    carry = entries.get(CARRY_PART)
    if carry is not None:
        src = _restore_carry(carry)
        return src, [ImportReport("gridmd source carry", "restored", "byte-exact round-trip")]
    return _native_import(entries)


def _restore_carry(raw: bytes) -> str:
    try:
        root = _parse_xml(raw.decode("utf-8"))
    except ET.ParseError as e:
        raise ValueError(f"malformed carry part: {e}") from e
    source_el = root.find("source")
    if source_el is None:
        raise ValueError("carry part missing <source>")
    if source_el.get("encoding") != "base64":
        raise ValueError(f"unsupported carry encoding {source_el.get('encoding')!r}")
    payload = re.sub(r"\s+", "", source_el.text or "")
    try:
        return base64.b64decode(payload).decode("utf-8")
    except (ValueError, UnicodeDecodeError) as e:
        raise ValueError(f"corrupt carry payload: {e}") from e


def _native_import(entries: dict[str, bytes]) -> tuple[str, list[ImportReport]]:
    def get(name: str) -> str | None:
        data = entries.get(name)
        return data.decode("utf-8") if data is not None else None

    wb_src = get("xl/workbook.xml")
    if wb_src is None:
        raise ValueError("missing xl/workbook.xml — not a spreadsheet package")
    wb = _parse_xml(wb_src)
    rels_src = get("xl/_rels/workbook.xml.rels")
    rel_targets: dict[str, str] = {}
    if rels_src is not None:
        for r in _parse_xml(rels_src).findall("Relationship"):
            rid, target = r.get("Id"), r.get("Target")
            if rid and target:
                rel_targets[rid] = target
    shared_src = get("xl/sharedStrings.xml")
    shared = _read_shared_strings(shared_src) if shared_src is not None else []

    report: list[ImportReport] = []
    out = ['---\ngridmd: "0.1"\n---\n']

    sheets_el = wb.find("sheets")
    sheet_nodes = sheets_el.findall("sheet") if sheets_el is not None else []
    for sh in sheet_nodes:
        name = sh.get("name", "Sheet")
        state = sh.get("state")
        rid = sh.get("id")  # r:id after namespace stripping
        target = _normalize_target(rel_targets[rid]) if rid in rel_targets else None
        out.append(f"\n# {name}\n")
        if state:
            vis = "very" if state == "veryHidden" else "true"
            out.append(f"\n```{{sheet}}\nhidden: {vis}\n```\n")
        if target is not None:
            ws_src = get(target)
            if ws_src is not None:
                _emit_worksheet(ws_src, shared, out)
            else:
                report.append(ImportReport(f"{name} worksheet", "not-emitted", "worksheet part missing"))

    report.append(ImportReport(f"{len(sheet_nodes)} sheet(s)", "imported"))
    report.append(ImportReport("worksheet core", "imported", "cells, formulas, merges reverse-parsed"))
    _report_untranslated(entries, report)
    return "".join(out), report


def _report_untranslated(entries: dict[str, bytes], report: list[ImportReport]) -> None:
    """Honestly note Tier-2 objects a foreign file carries that the native
    worksheet-core import does not translate (SPEC §11: never silently claim)."""
    seen: dict[str, str] = {
        "charts": "charts/",
        "pivotTables": "pivotTables/",
        "slicers": "slicers/",
        "media": "media/",
        "comments": "comments",
        "threadedComments": "threadedComments/",
    }
    for feature, needle in seen.items():
        if any(needle in name for name in entries):
            report.append(ImportReport(feature, "not-imported", "present in source package; not reverse-parsed"))


def _normalize_target(t: str) -> str:
    t = t[1:] if t.startswith("/") else t
    return t if t.startswith("xl/") else f"xl/{t}"


def _read_shared_strings(src: str) -> list[str]:
    return ["".join(si.itertext()) for si in _parse_xml(src).findall("si")]


def _emit_worksheet(src: str, shared: list[str], out: list[str]) -> None:
    ws = _parse_xml(src)
    sheet_data = ws.find("sheetData")
    if sheet_data is None:
        return
    cells: list[tuple[int, int, str]] = []
    for row in sheet_data.findall("row"):
        for c in row.findall("c"):
            pos = parse_cell(c.get("r", ""))
            if pos is None:
                continue
            line = _cell_to_at(c, shared, pos.col, pos.row)
            if line is not None:
                cells.append((pos.row, pos.col, line))
    cells.sort(key=lambda x: (x[0], x[1]))
    for _, _, line in cells:
        out.append(line + "\n")
    merges = ws.find("mergeCells")
    if merges is not None:
        for m in merges.findall("mergeCell"):
            refr = m.get("ref")
            if refr:
                out.append(f"@ {refr} {{ merge: true }}\n")


def _cell_to_at(c: ET.Element, shared: list[str], col: int, row: int) -> str | None:
    refr = f"{num_to_col(col)}{row}"
    t = c.get("t")
    v_el = c.find("v")
    v = v_el.text if v_el is not None else None
    f_el = c.find("f")
    if f_el is not None:
        formula = f_el.text or ""
        if formula == "":
            return None
        if v is None:
            return f"@ {refr} ={formula}"
        cached = _quote_text(v) if t in ("str", "s") else ("TRUE" if v == "1" else "FALSE") if t == "b" else v
        return f"@ {refr} ={formula} :: {cached}"
    if t == "s":
        if v is None:
            return None
        idx = int(v)
        if idx >= len(shared):
            return None
        return _emit_text(refr, shared[idx])
    if t == "inlineStr":
        is_el = c.find("is")
        text = "".join(is_el.itertext()) if is_el is not None else ""
        return _emit_text(refr, text)
    if t == "str":
        return _emit_text(refr, v or "")
    if t == "b":
        if v is None:
            return None
        return f"@ {refr} {'TRUE' if v == '1' else 'FALSE'}"
    if t == "e":
        return None if v is None else f"@ {refr} {v}"
    if v is None:
        return None
    return f"@ {refr} {v}"


def _emit_text(refr: str, text: str) -> str:
    if "\n" in text or "\r" in text:
        return f"@ {refr}\n  value: {_yaml_dq(text)}"
    return f"@ {refr} {_quote_text(text)}"


def _quote_text(text: str) -> str:
    return '"' + text.replace('"', '""') + '"'


def _yaml_dq(text: str) -> str:
    out = ['"']
    for ch in text:
        if ch == '"':
            out.append('\\"')
        elif ch == "\\":
            out.append("\\\\")
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif ord(ch) < 0x20:
            out.append(f"\\x{ord(ch):02x}")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def _parse_xml(text: str) -> ET.Element:
    # stdlib-only mandate → no defusedxml; reject DTDs so entity-expansion /
    # external-entity (billion-laughs / XXE) inputs never reach expat. Valid
    # OOXML parts never declare a DOCTYPE or custom entities.
    if re.search(r"<!(DOCTYPE|ENTITY)", text, re.I):
        raise ValueError("XML declares a DTD/entity — rejected (unsupported in OOXML parts)")
    root = ET.fromstring(text)
    for el in root.iter():
        if "}" in el.tag:
            el.tag = el.tag.split("}", 1)[1]
        if el.attrib:
            new_attrs = {(k.split("}", 1)[1] if "}" in k else k): v for k, v in el.attrib.items()}
            el.attrib.clear()
            el.attrib.update(new_attrs)
    return root
