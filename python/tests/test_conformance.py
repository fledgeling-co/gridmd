"""The three conformance laws (conformance/README.md) + the foreign-xlsx bonus."""

from __future__ import annotations

import pytest

from gridmd import (
    GridmdError,
    build_workbook_model,
    dump,
    dump_model,
    lint,
    write_xlsx,
    xlsx_to_gridmd,
)

from .conftest import FIXTURES_XLSX, INVALID, VALID_CASES


def test_law1_dump_byte_identical(valid_case):
    """Law 1: parse+dump is byte-identical to the expected canonical dump."""
    _name, src, expected = valid_case
    source = src.read_text(encoding="utf-8")
    result = lint(source)
    assert result.errors == []
    produced = dump(source)
    assert produced == expected.read_text(encoding="utf-8")


@pytest.mark.parametrize("path", sorted(INVALID.glob("*.gmd")), ids=lambda p: p.name)
def test_law2_reject_invalid(path):
    """Law 2: every invalid fixture fails strict validation with >=1 error."""
    source = path.read_text(encoding="utf-8")
    result = lint(source)
    assert result.errors, f"{path.name} should have been rejected"
    with pytest.raises(GridmdError):
        dump(source)


def test_law3_round_trip(valid_case):
    """Law 3: dump(import(export(doc))) == dump(doc)."""
    _name, src, _expected = valid_case
    source_bytes = src.read_bytes()
    source = source_bytes.decode("utf-8")
    original_dump = dump(source)

    model = build_workbook_model(lint(source).doc)
    xlsx_bytes, report = write_xlsx(model, source_bytes)
    assert report  # loud fidelity report
    restored, import_report = xlsx_to_gridmd(xlsx_bytes)
    assert import_report[0].action == "restored"
    round_tripped = dump_model(build_workbook_model(lint(restored).doc))
    assert round_tripped == original_dump


def test_bonus_foreign_xlsx_imports_lint_clean():
    """Bonus: native import of the committed JS-written foreign xlsx (DEFLATE,
    no carry part) produces GridMD that lints clean under the strict linter."""
    foreign = FIXTURES_XLSX / "quarterly-report.xlsx"
    gmd, report = xlsx_to_gridmd(foreign.read_bytes())
    assert all(r.action != "restored" for r in report)  # native path, not carry
    result = lint(gmd)
    assert result.errors == [], result.errors
    # and it must be dumpable
    assert dump(gmd).endswith("}\n")


def test_all_four_names_present():
    assert set(VALID_CASES) == {"01-cells", "02-structure", "03-features", "quarterly-report"}
