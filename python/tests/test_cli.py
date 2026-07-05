"""CLI verbs: dump | to-xlsx | from-xlsx."""

from __future__ import annotations

import io

from gridmd.cli import main
from gridmd.zipio import zip_write

from .conftest import EXPECTED, FIXTURES, FIXTURES_XLSX, VALID_CASES


def _run(argv):
    out, err = io.StringIO(), io.StringIO()
    code = main(argv, stdout=out, stderr=err)
    return code, out.getvalue(), err.getvalue()


def test_dump_success():
    src, expected = VALID_CASES["01-cells"]
    code, out, _err = _run(["dump", str(src)])
    assert code == 0
    assert out == expected.read_text(encoding="utf-8")


def test_dump_invalid(tmp_path):
    bad = tmp_path / "bad.gmd"
    bad.write_text('---\ngridmd: "0.1"\n---\n# S\n@ A1 1\n@ A1 2\n')
    code, _out, err = _run(["dump", str(bad)])
    assert code == 1 and "cell defined more than once" in err


def test_dump_missing_file():
    code, _out, err = _run(["dump", "/no/such/file.gmd"])
    assert code == 1 and err.startswith("error:")


def test_to_xlsx_and_round_trip(tmp_path):
    src = FIXTURES / "01-cells.gmd"
    xlsx = tmp_path / "out.xlsx"
    code, _out, err = _run(["to-xlsx", str(src), "-o", str(xlsx)])
    assert code == 0 and xlsx.exists()
    assert "lossless round-trip" in err

    back = tmp_path / "back.gmd"
    code, _out, err = _run(["from-xlsx", str(xlsx), "-o", str(back)])
    assert code == 0 and "restored" in err

    dumped = tmp_path / "d.json"
    c2, out2, _ = _run(["dump", str(back)])
    dumped.write_text(out2)
    assert out2 == (EXPECTED / "01-cells.json").read_text(encoding="utf-8")


def test_to_xlsx_invalid(tmp_path):
    bad = tmp_path / "bad.gmd"
    bad.write_text('---\ngridmd: "0.1"\n---\n# S\n@ A1 1\n@ A1 2\n')
    code, _out, err = _run(["to-xlsx", str(bad), "-o", str(tmp_path / "x.xlsx")])
    assert code == 1 and "defined more than once" in err


def test_from_xlsx_foreign_native(tmp_path):
    out = tmp_path / "foreign.gmd"
    code, _o, err = _run(["from-xlsx", str(FIXTURES_XLSX / "quarterly-report.xlsx"), "-o", str(out)])
    assert code == 0
    assert "worksheet core: imported" in err
    assert "not-imported" in err  # note-less + noted report entries both printed


def test_from_xlsx_self_check_failure(tmp_path):
    # A worksheet whose sheet name carries a forbidden ':' → the reverse-parsed
    # GridMD fails strict lint, so from-xlsx must report and exit non-zero.
    wb = (
        '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="Bad:Name" sheetId="1" r:id="rId1"/></sheets></workbook>'
    )
    xlsx = tmp_path / "bad.xlsx"
    xlsx.write_bytes(zip_write([("xl/workbook.xml", wb.encode("utf-8"))]))
    code, _o, err = _run(["from-xlsx", str(xlsx), "-o", str(tmp_path / "o.gmd")])
    assert code == 1 and "self-check" in err


def test_default_streams(capsys):
    src, _expected = VALID_CASES["01-cells"]
    code = main(["dump", str(src)])
    captured = capsys.readouterr()
    assert code == 0 and captured.out.endswith("}\n")
