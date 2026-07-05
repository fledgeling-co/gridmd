"""A1-reference parsing (refs)."""

from __future__ import annotations

import pytest

from gridmd.refs import (
    MAX_COL,
    col_to_num,
    num_to_col,
    parse_cell,
    parse_target,
    ref_key,
)


@pytest.mark.parametrize("letters,n", [("A", 1), ("Z", 26), ("AA", 27), ("XFD", MAX_COL)])
def test_col_num_roundtrip(letters, n):
    assert col_to_num(letters) == n
    assert num_to_col(n) == letters


def test_parse_cell_valid():
    c = parse_cell("B3")
    assert c is not None and (c.col, c.row) == (2, 3)


@pytest.mark.parametrize("text", ["A0", "1A", "AAAA1", "A1048577", "", "B"])
def test_parse_cell_invalid(text):
    assert parse_cell(text) is None


def test_parse_target_cell():
    t = parse_target("A1")
    assert t.kind == "cell" and (t.c1, t.r1, t.c2, t.r2) == (1, 1, 1, 1)


def test_parse_target_range_normalized():
    t = parse_target("C3:A1")
    assert t.kind == "range" and (t.c1, t.r1, t.c2, t.r2) == (1, 1, 3, 3)


def test_parse_target_cols_and_rows():
    cols = parse_target("B:A")
    assert cols.kind == "cols" and (cols.c1, cols.c2) == (1, 2)
    rows = parse_target("3:1")
    assert rows.kind == "rows" and (rows.r1, rows.r2) == (1, 3)


def test_parse_target_sheet_qualified():
    t = parse_target("Sheet1!A1")
    assert t.sheet == "Sheet1" and t.kind == "cell"


def test_parse_target_quoted_sheet():
    t = parse_target("'My ''Sheet'''!A1")
    assert t.sheet == "My 'Sheet'"


@pytest.mark.parametrize("text", ["garbage", "A1:B2:C3", "A1:garbage", "A:ZZZ", "1:9999999", ":"])
def test_parse_target_invalid(text):
    assert parse_target(text) is None


def test_ref_key():
    assert ref_key(2, 3) == "2,3"
