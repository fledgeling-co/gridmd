"""A1-reference parsing (SPEC.md §8.2, Appendix A)."""

from __future__ import annotations

import re
from dataclasses import dataclass

MAX_COL = 16384  # XFD
MAX_ROW = 1048576


@dataclass(frozen=True)
class CellPos:
    col: int
    row: int


@dataclass
class Target:
    kind: str  # "cell" | "range" | "cols" | "rows"
    sheet: str | None
    c1: int
    r1: int
    c2: int
    r2: int
    line: int | None = None


def col_to_num(letters: str) -> int:
    n = 0
    for ch in letters:
        n = n * 26 + (ord(ch) - 64)
    return n


def num_to_col(n: int) -> str:
    s = ""
    while n > 0:
        r = (n - 1) % 26
        s = chr(65 + r) + s
        n = (n - 1 - r) // 26
    return s


_CELL_RE = re.compile(r"^(\$?)([A-Z]{1,3})(\$?)([1-9]\d{0,6})$")
_COLRANGE_RE = re.compile(r"^\$?([A-Z]{1,3}):\$?([A-Z]{1,3})$")
_ROWRANGE_RE = re.compile(r"^\$?([1-9]\d{0,6}):\$?([1-9]\d{0,6})$")


def parse_cell(text: str) -> CellPos | None:
    m = _CELL_RE.match(text)
    if not m:
        return None
    col = col_to_num(m.group(2))
    row = int(m.group(4))
    if col > MAX_COL or row > MAX_ROW:
        return None
    return CellPos(col=col, row=row)


def parse_target(input_text: str) -> Target | None:
    """Parse a target: ``cell`` | ``cell:cell`` | ``col:col`` | ``row:row``,
    with an optional leading ``Sheet!`` qualifier ('quoted' names supported)."""
    text = input_text
    sheet: str | None = None
    bang = text.rfind("!")
    if bang != -1:
        sheet = text[:bang]
        if sheet.startswith("'") and sheet.endswith("'") and len(sheet) >= 2:
            sheet = sheet[1:-1].replace("''", "'")
        text = text[bang + 1 :]

    cell = parse_cell(text)
    if cell:
        return Target("cell", sheet, cell.col, cell.row, cell.col, cell.row)
    if ":" in text:
        parts = text.split(":")
        if len(parts) == 2:
            a = parse_cell(parts[0])
            b = parse_cell(parts[1])
            if a and b:
                return Target(
                    "range", sheet,
                    min(a.col, b.col), min(a.row, b.row),
                    max(a.col, b.col), max(a.row, b.row),
                )
            m = _COLRANGE_RE.match(text)
            if m:
                c1, c2 = col_to_num(m.group(1)), col_to_num(m.group(2))
                if c1 <= MAX_COL and c2 <= MAX_COL:
                    return Target("cols", sheet, min(c1, c2), 1, max(c1, c2), MAX_ROW)
            m = _ROWRANGE_RE.match(text)
            if m:
                r1, r2 = int(m.group(1)), int(m.group(2))
                if r1 <= MAX_ROW and r2 <= MAX_ROW:
                    return Target("rows", sheet, 1, min(r1, r2), MAX_COL, max(r1, r2))
    return None


def ref_key(col: int, row: int) -> str:
    return f"{col},{row}"
