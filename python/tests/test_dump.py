"""Canonical dump serialization edge cases (dump)."""

from __future__ import annotations

import pytest

from gridmd import build_workbook_model, dump, lint
from gridmd.dump import _serialize, _write_json_string, dump_model
from gridmd.model import Sheet, WorkbookModel


def _dump(body: str) -> str:
    return dump('---\ngridmd: "0.1"\n---\n# S\n' + body)


def test_string_escaping():
    buf: list[str] = []
    _write_json_string(buf, 'a"b\\c\n\t\r\x00é')
    assert "".join(buf) == '"a\\"b\\\\c\\n\\t\\r\\u0000é"'


def test_serialize_rejects_unknown_type():
    with pytest.raises(TypeError):
        _serialize([], object(), 0)


def test_empty_containers():
    buf: list[str] = []
    _serialize(buf, {}, 0)
    _serialize(buf, [], 0)
    assert "".join(buf) == "{}[]"


def test_number_in_dump():
    out = _dump("@ A1 0.3\n")
    assert '"v": 0.3' in out


def test_names_value_stringify():
    src = (
        "---\n"
        'gridmd: "0.1"\n'
        "names:\n"
        "  - { name: A, value: 42 }\n"
        "  - { name: B, value: true }\n"
        "  - { name: C, ref: S!A1 }\n"
        "---\n# S\n@ A1 1\n"
    )
    out = dump(src)
    assert '"value": "42"' in out
    assert '"value": "true"' in out
    assert '"ref": "S!A1"' in out


def test_direct_model_edge_cases():
    # freeze non-string → null; protect non-dict → false; hidden 'very'; no gridmd
    sheet = Sheet(name="X", meta={"freeze": 123, "protect": "nope", "hidden": "very"}, kind="worksheet")
    model = WorkbookModel(fm={}, sheets=[sheet])
    out = dump_model(model)
    assert '"gridmd": null' in out
    assert '"hidden": "very"' in out
    assert '"freeze": null' in out
    assert '"protected": false' in out


def test_rich_missing_text_treated_as_empty():
    sheet = Sheet(name="X", meta={}, kind="worksheet")
    from gridmd.model import Cell, CellContent

    sheet.cells["1,1"] = Cell(col=1, row=1, content=CellContent(rich=[{"color": "#000"}, {"text": "b"}]))
    out = dump_model(WorkbookModel(fm={"gridmd": "0.1"}, sheets=[sheet]))
    assert '"t": "rich"' in out and '"v": "b"' in out


def test_dump_roundtrips_via_lint():
    result = lint('---\ngridmd: "0.1"\n---\n# S\n@ A1 1\n')
    assert dump_model(build_workbook_model(result.doc)).endswith("}\n")


def test_names_null_value_and_non_dict_entries():
    # value present-but-null → String(null) == "null"; a non-dict entry is skipped
    model = WorkbookModel(fm={"gridmd": "0.1", "names": [5, {"name": "A", "value": None}]}, sheets=[])
    out = dump_model(model)
    assert '"value": "null"' in out
    assert '"name": "A"' in out
