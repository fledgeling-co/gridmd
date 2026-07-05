"""Block tree → workbook model, and relative-fill formula translation."""

from __future__ import annotations

from gridmd import build_workbook_model, lint
from gridmd.model import translate_formula
from gridmd.parser import parse_document


def _model(body_lines: str):
    src = '---\ngridmd: "0.1"\n---\n# S\n' + body_lines
    result = lint(src)
    assert result.errors == [], result.errors
    return build_workbook_model(result.doc).sheets[0]


def _raw(body_lines: str):
    """Materialize an UNVALIDATED document — exercises the model's defensive
    guards for malformed blocks that strict lint would otherwise reject."""
    doc = parse_document('---\ngridmd: "0.1"\n---\n# S\n' + body_lines)
    return build_workbook_model(doc).sheets[0]


def _cell(sheet, ref_col, ref_row):
    from gridmd.refs import ref_key

    return sheet.cells[ref_key(ref_col, ref_row)].content


def test_grid_and_scalar_content():
    s = _model("```{grid} A1\n| 1 | text |\n```\n")
    assert _cell(s, 1, 1).scalar.value == 1.0
    assert _cell(s, 2, 1).scalar.value == "text"


def test_body_value_content():
    s = _model("@ A1\n  value: 2026-07-04\n")
    assert _cell(s, 1, 1).scalar.kind == "date"


def test_body_formula_with_cache_and_spill():
    s = _model("@ A1\n  formula: =SORT(B1:B3)\n  value: 5\n  spill: A1:A3\n")
    c = _cell(s, 1, 1)
    assert c.formula == "SORT(B1:B3)" and c.array_ref == "A1:A3"
    assert c.cached.value == 5.0


def test_body_array_sets_cse():
    s = _model("@ A1\n  formula: =X\n  array: A1:A2\n")
    assert _cell(s, 1, 1).cse is True


def test_entity_cell_emits_display_text():
    s = _model('@ A1\n  entity: { type: stock, id: "X:Y", text: "MSFT" }\n  fields: { Price: 1 }\n')
    c = _cell(s, 1, 1)
    assert c.scalar.value == "MSFT" and c.entity_fields == {"Price": 1}


def test_entity_falls_back_to_id_then_empty():
    s1 = _model('@ A1\n  entity: { id: "OnlyId" }\n')
    assert _cell(s1, 1, 1).scalar.value == "OnlyId"
    s2 = _model("@ A1\n  entity: {}\n")
    assert _cell(s2, 1, 1).scalar.value == ""


def test_rich_content():
    s = _model('@ A1\n  rich:\n    - { text: "a" }\n    - { text: "b" }\n')
    assert _cell(s, 1, 1).rich is not None


def test_inline_formula_with_spill_prop():
    s = _model("@ A1 =SORT(B1:B3) { spill: A1:A3 }\n")
    c = _cell(s, 1, 1)
    assert c.formula == "SORT(B1:B3)" and c.array_ref == "A1:A3"


def test_range_formula_relative_fill():
    s = _model("@ A1:A3 =B1*2\n")
    assert _cell(s, 1, 1).formula == "B1*2"
    assert _cell(s, 1, 2).formula == "B2*2"
    assert _cell(s, 1, 3).formula == "B3*2"


def test_spill_cache_fills_owner_cache():
    src = "@ A1 =SORT(B1:B3) { spill: A1:A3 }\n```{spill-cache} A1\n| AU |\n| NZ |\n| UK |\n```\n"
    s = _model(src)
    assert _cell(s, 1, 1).cached.value == "AU"
    assert _cell(s, 1, 2).scalar.value == "NZ"


def test_table_total_unknown_column_skipped():
    src = (
        "```{table} T at A1\n"
        "total:\n  qty: =SUBTOTAL(109,[qty])\n"
        "---\n| item | qty |\n| a | 1 |\n```\n"
    )
    s = _model(src)
    assert s.tables[0].name == "T" and s.tables[0].body_rows == 1


def test_shape_and_textbox_counts():
    s = _model("```{shape} rect at A1\n```\n```{textbox} at B2\ntext: hi\n```\n")
    assert len(s.shapes) == 2
    assert s.shapes[0]["preset"] == "rect" and s.shapes[1]["preset"] == "textbox"


def test_merge_and_link_and_note():
    src = (
        '@ A1:B1 { merge: true }\n'
        '@ A2 "x" { link: "https://e.com", tip: "t" }\n'
        '@ A3\n  note: hello\n'
    )
    s = _model(src)
    assert len(s.merges) == 1
    assert s.hyperlinks[0]["target"] == "https://e.com"
    assert s.notes[0]["text"] == "hello"


# ---- translate_formula ----
def test_translate_shifts_relative():
    assert translate_formula("A1", 1, 1) == "B2"
    assert translate_formula("SUM(A1)", 1, 1) == "SUM(B2)"


def test_translate_respects_absolute():
    assert translate_formula("$A$1", 3, 3) == "$A$1"
    assert translate_formula("$A1", 1, 1) == "$A2"
    assert translate_formula("A$1", 1, 1) == "B$1"


def test_translate_skips_string_literals():
    assert translate_formula('"A1"&A1', 0, 1) == '"A1"&B1'
    assert translate_formula("'Sheet A1'&A1", 0, 1) == "'Sheet A1'&B1"
    assert translate_formula('"A1"&A1', 1, 0) == '"A1"&A2'


def test_translate_clamps_to_one():
    assert translate_formula("A1", -5, -5) == "A1"


def test_translate_doubled_quote_in_literal():
    assert translate_formula('"a""b"&A1', 0, 1) == '"a""b"&B1'


def test_body_value_bool_time_text():
    assert _cell(_model("@ A1\n  value: true\n"), 1, 1).scalar.kind == "boolean"
    assert _cell(_model("@ A1\n  value: 12:30\n"), 1, 1).scalar.kind == "time"
    assert _cell(_model("@ A1\n  value: plaintext\n"), 1, 1).scalar.kind == "text"


def test_inline_formula_array_sets_cse():
    assert _cell(_model("@ A1 =X { array: A1:A2 }\n"), 1, 1).cse is True


# ---- model defensive guards on unvalidated input ----
def test_raw_grid_without_anchor_produces_no_cells():
    assert _raw("```{grid}\n| 1 |\n```\n").cells == {}


def test_raw_spill_cache_without_anchor_and_blank_cell():
    assert _raw("```{spill-cache}\n| 1 |\n```\n").cells == {}
    s = _raw("```{spill-cache} A1\n| a |  |\n| b | c |\n```\n")
    assert s.cells  # first cell became the owner cache; the blank cell skipped


def test_raw_table_without_anchor_is_dropped():
    assert _raw("```{table} T\n---\n| a |\n| 1 |\n```\n").tables == []


def test_raw_table_total_unknown_column_skipped():
    s = _raw("```{table} T at A1\ntotal:\n  gone: X\n---\n| a |\n| 1 |\n```\n")
    assert s.tables[0].name == "T"


def test_raw_invalid_at_target_ignored():
    assert _raw("@ notatarget 1\n").cells == {}
