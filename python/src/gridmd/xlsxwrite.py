"""XLSX package writer: GridMD model → ``.xlsx`` bytes.

The worksheet core (cell values, formulas + typed cached values, ISO→serial
dates via the 1900 phantom-leap rule, inline strings, and merges) is emitted
natively so the file opens as a real spreadsheet. The complete GridMD source is
additionally carried, base64-encoded, in ``customXml/gridmdCarry.xml`` so every
feature the canonical dump measures round-trips losslessly — nothing is silently
dropped (SPEC §11). See the README for the carry design.
"""

from __future__ import annotations

import base64
from datetime import date

from .model import Cell, CellContent, Sheet, WorkbookModel
from .numfmt import format_number
from .refs import num_to_col
from .scalar import Scalar

CARRY_PART = "customXml/gridmdCarry.xml"
_XML_DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'


def write_xlsx(model: WorkbookModel, source: bytes) -> tuple[bytes, list[str]]:
    """Render ``model`` (materialized from ``source``) to ``.xlsx`` bytes and a
    human-readable fidelity report."""
    from .zipio import zip_write

    parts, report = _build_parts(model, source)
    return zip_write([(name, data.encode("utf-8")) for name, data in parts]), report


def _build_parts(model: WorkbookModel, source: bytes) -> tuple[list[tuple[str, str]], list[str]]:
    date_system = 1904 if model.fm.get("date-system") == 1904 else 1900
    parts: list[tuple[str, str]] = [
        ("[Content_Types].xml", _content_types(len(model.sheets))),
        ("_rels/.rels", _root_rels()),
        ("xl/workbook.xml", _workbook_xml(model)),
        ("xl/_rels/workbook.xml.rels", _workbook_rels(len(model.sheets))),
        ("xl/styles.xml", _styles_xml()),
    ]
    report: list[str] = []
    native_cells = 0
    for i, s in enumerate(model.sheets):
        xml, cells = _worksheet_xml(s, date_system)
        native_cells += cells
        parts.append((f"xl/worksheets/sheet{i + 1}.xml", xml))
        report.append(f"sheet {s.name!r}: {cells} cells, {len(s.merges)} merges emitted natively")
        carried = _carried_summary(s)
        if carried:
            report.append("  carried via source part: " + carried)
    parts.append((CARRY_PART, _carry_xml(base64.b64encode(source).decode("ascii"))))
    report.append(f"{native_cells} cells written natively across {len(model.sheets)} sheet(s)")
    report.append(f"full GridMD source carried in {CARRY_PART} (lossless round-trip; nothing dropped)")
    return parts, report


def _carried_summary(s: Sheet) -> str:
    parts: list[str] = []
    for label, n in (
        ("cf", sum(len(r) if isinstance(r, list) else 0 for r in s.cf)),
        ("validations", len(s.validations)),
        ("notes", len(s.notes)),
        ("threads", len(s.threads)),
        ("scenarios", len(s.scenarios)),
        ("sparklines", len(s.sparklines)),
        ("charts", len(s.charts)),
        ("pivots", len(s.pivots)),
        ("slicers", len(s.slicers)),
        ("images", len(s.images)),
        ("shapes", len(s.shapes)),
        ("hyperlinks", len(s.hyperlinks)),
        ("tables", len(s.tables)),
    ):
        if n > 0:
            parts.append(f"{label}={n}")
    return " ".join(parts)


def _content_types(n_sheets: int) -> str:
    overrides = [
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>',
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>',
    ]
    for i in range(n_sheets):
        overrides.append(
            f'<Override PartName="/xl/worksheets/sheet{i + 1}.xml" '
            'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        )
    return (
        _XML_DECL
        + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        + '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        + '<Default Extension="xml" ContentType="application/xml"/>'
        + "".join(overrides)
        + "</Types>"
    )


def _root_rels() -> str:
    return (
        _XML_DECL
        + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        + f'<Relationship Id="rIdGmd" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="{CARRY_PART}"/>'
        + "</Relationships>"
    )


def _workbook_xml(model: WorkbookModel) -> str:
    sheet_els = []
    for i, s in enumerate(model.sheets):
        hidden = s.meta.get("hidden")
        state = ' state="hidden"' if hidden is True else ' state="veryHidden"' if hidden == "very" else ""
        sheet_els.append(f'<sheet name="{_esc(s.name)}" sheetId="{i + 1}"{state} r:id="rId{i + 1}"/>')
    return (
        _XML_DECL
        + '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>'
        + "".join(sheet_els)
        + "</sheets></workbook>"
    )


def _workbook_rels(n_sheets: int) -> str:
    rels = [
        f'<Relationship Id="rId{i + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i + 1}.xml"/>'
        for i in range(n_sheets)
    ]
    rels.append(
        f'<Relationship Id="rId{n_sheets + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
    )
    return (
        _XML_DECL
        + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + "".join(rels)
        + "</Relationships>"
    )


def _styles_xml() -> str:
    return (
        _XML_DECL
        + '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        + '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
        + '<fills count="1"><fill><patternFill patternType="none"/></fill></fills>'
        + "<borders count=\"1\"><border/></borders>"
        + '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        + '<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>'
        + '<xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs>'
        + "</styleSheet>"
    )


def _worksheet_xml(s: Sheet, date_system: int) -> tuple[str, int]:
    rows: dict[int, list[Cell]] = {}
    for c in s.cells.values():
        rows.setdefault(c.row, []).append(c)

    body: list[str] = []
    count = 0
    for r in sorted(rows):
        cells = sorted(rows[r], key=lambda c: c.col)
        body.append(f'<row r="{r}">')
        for c in cells:
            frag = _cell_xml(c, date_system)
            if frag:
                body.append(frag)
                count += 1
        body.append("</row>")

    merges = ""
    if s.merges:
        parts = "".join(
            f'<mergeCell ref="{num_to_col(m.c1)}{m.r1}:{num_to_col(m.c2)}{m.r2}"/>' for m in s.merges
        )
        merges = f'<mergeCells count="{len(s.merges)}">{parts}</mergeCells>'

    xml = (
        _XML_DECL
        + '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
        + "".join(body)
        + "</sheetData>"
        + merges
        + "</worksheet>"
    )
    return xml, count


def _cell_xml(c: Cell, date_system: int) -> str:
    ref = f"{num_to_col(c.col)}{c.row}"
    ct: CellContent = c.content  # invariant: every materialized cell has content
    if ct.rich:
        text = "".join("" if r.get("text") is None else str(r.get("text")) for r in ct.rich)
        return _inline_str(ref, text)
    if ct.formula is not None:
        t = _cached_type(ct.cached)
        t_attr = f' t="{t}"' if t else ""
        v = _cached_value(ct.cached, date_system)
        v_el = f"<v>{v}</v>" if v != "" else ""
        return f'<c r="{ref}"{t_attr}><f>{_esc(ct.formula)}</f>{v_el}</c>'
    return _scalar_cell(ref, ct.scalar, date_system)


def _scalar_cell(ref: str, sc: Scalar | None, date_system: int) -> str:
    if sc is None:
        return ""
    if sc.kind == "number":
        return f'<c r="{ref}"><v>{format_number(sc.value)}</v></c>'
    if sc.kind == "boolean":
        return f'<c r="{ref}" t="b"><v>{1 if sc.value else 0}</v></c>'
    if sc.kind == "error":
        return f'<c r="{ref}" t="e"><v>{_esc(sc.value)}</v></c>'
    if sc.kind in ("date", "time"):
        return f'<c r="{ref}" s="1"><v>{format_number(_iso_to_serial(str(sc.value), date_system))}</v></c>'
    return _inline_str(ref, str(sc.value))


def _inline_str(ref: str, text: str) -> str:
    return f'<c r="{ref}" t="inlineStr"><is><t xml:space="preserve">{_esc(text)}</t></is></c>'


def _cached_type(sc: Scalar | None) -> str:
    if sc is None:
        return ""
    return {"text": "str", "boolean": "b", "error": "e"}.get(sc.kind, "")


def _cached_value(sc: Scalar | None, date_system: int) -> str:
    if sc is None:
        return ""
    if sc.kind == "number":
        return format_number(sc.value)
    if sc.kind == "boolean":
        return "1" if sc.value else "0"
    if sc.kind == "error":
        return _esc(sc.value)
    if sc.kind in ("date", "time"):
        return format_number(_iso_to_serial(str(sc.value), date_system))
    if sc.kind == "text":
        return _esc(sc.value)
    return ""


def _carry_xml(b64: str) -> str:
    return (
        _XML_DECL
        + '<gridmd xmlns="urn:gridmd:carry" version="0.1"><source encoding="base64">'
        + b64
        + "</source></gridmd>"
    )


def _esc(s: object) -> str:
    return (
        str(s)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
        .replace("'", "&apos;")
    )


def _iso_to_serial(iso: str, date_system: int) -> float:
    """ISO date/time → Excel serial (INTEROP §2), reproducing the 1900
    phantom-leap-day offset."""
    if len(iso) >= 3 and iso[2] == ":":
        date_part, time_part = "", iso
    elif "T" in iso:
        date_part, time_part = iso.split("T", 1)
    else:
        date_part, time_part = iso, ""

    frac = 0.0
    if time_part:
        bits = time_part.split(":")
        hh = int(bits[0])
        mm = int(bits[1]) if len(bits) > 1 else 0
        ss = int(bits[2]) if len(bits) > 2 else 0
        frac = (hh * 3600 + mm * 60 + ss) / 86400
    if not date_part:
        return frac

    y, m, d = (int(x) for x in date_part.split("-"))
    ordinal = date(y, m, d).toordinal()
    if date_system == 1904:
        days = ordinal - date(1904, 1, 1).toordinal()
    else:
        days = ordinal - date(1899, 12, 30).toordinal()
        if days < 61:
            days -= 1
    return days + frac
