"""XLSX writer, reader (carry + native), and deterministic ZIP I/O."""

from __future__ import annotations

import pytest

from gridmd import build_workbook_model, lint, write_xlsx, xlsx_to_gridmd
from gridmd.scalar import Scalar
from gridmd.xlsxread import _parse_xml, _yaml_dq
from gridmd.xlsxwrite import (
    CARRY_PART,
    _cached_type,
    _cached_value,
    _iso_to_serial,
    _scalar_cell,
)
from gridmd.zipio import zip_read, zip_write

XMLNS_WS = 'xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"'


def _xlsx_from(src: str) -> tuple[bytes, str]:
    result = lint(src)
    assert result.errors == [], result.errors
    buf, _report = write_xlsx(build_workbook_model(result.doc), src.encode("utf-8"))
    return buf, src


# ---- iso_to_serial ----
def test_iso_to_serial_dates_and_times():
    assert _iso_to_serial("1900-01-01", 1900) == 1  # phantom-leap adjustment
    assert _iso_to_serial("1900-02-28", 1900) == 59  # < 61 branch
    assert _iso_to_serial("1900-03-01", 1900) == 61  # boundary false branch
    assert _iso_to_serial("1904-01-02", 1904) == 1
    assert _iso_to_serial("12:30", 1900) == pytest.approx(0.520833333, rel=1e-6)
    assert _iso_to_serial("2026-07-04T06:00", 1900) == pytest.approx(46207.25, abs=0.01)
    assert _iso_to_serial("06:00:30", 1900) == pytest.approx((6 * 3600 + 30) / 86400, rel=1e-9)


# ---- writer cell types ----
def test_writer_emits_all_cell_types():
    src = (
        '---\ngridmd: "0.1"\n---\n# S\n'
        "@ A1 42\n@ A2 TRUE\n@ A3 #DIV/0!\n@ A4 2026-07-04\n@ A5 12:30\n@ A6 hello\n"
        '@ A7 =B1*2 :: 5\n@ A8 =B1 :: "text"\n@ A9 =B1 :: TRUE\n@ A10 =B1 :: #N/A\n'
        "@ A11 =B1\n@ A12 =B1 :: 2026-07-04\n"
        '@ B1\n  rich:\n    - { text: "x" }\n    - { text: "y" }\n'
    )
    buf, _ = _xlsx_from(src)
    parts = zip_read(buf)
    ws = parts["xl/worksheets/sheet1.xml"].decode("utf-8")
    assert "<v>42</v>" in ws
    assert '<c r="A2" t="b"><v>1</v></c>' in ws
    assert '<c r="A3" t="e"><v>#DIV/0!</v></c>' in ws
    assert '<c r="A4" s="1">' in ws
    assert '<c r="A6" t="inlineStr"><is><t xml:space="preserve">hello</t></is></c>' in ws
    assert "<f>B1*2</f><v>5</v>" in ws
    assert '<c r="A8" t="str"><f>B1</f><v>text</v></c>' in ws
    assert '<c r="A9" t="b"><f>B1</f><v>1</v></c>' in ws
    assert '<c r="A10" t="e"><f>B1</f><v>#N/A</v></c>' in ws
    assert '<c r="A11"><f>B1</f></c>' in ws  # no cached value
    assert '<c r="B1" t="inlineStr"><is><t xml:space="preserve">xy</t></is></c>' in ws


def test_writer_merges_and_hidden_states():
    src = (
        '---\ngridmd: "0.1"\n---\n# Visible\n@ A1:B1 { merge: true }\n@ A1 1\n'
        "# Hidden\n```{sheet}\nhidden: true\n```\n@ A1 1\n"
        "# Gone\n```{sheet}\nhidden: very\n```\n@ A1 1\n"
    )
    buf, _ = _xlsx_from(src)
    parts = zip_read(buf)
    ws = parts["xl/worksheets/sheet1.xml"].decode("utf-8")
    assert '<mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells>' in ws
    wb = parts["xl/workbook.xml"].decode("utf-8")
    assert 'state="hidden"' in wb and 'state="veryHidden"' in wb


def test_writer_report_lists_carried_features():
    src = (
        '---\ngridmd: "0.1"\n---\n# S\n@ A1 1\n'
        '```{comments} A1\n- { by: x, at: y, text: z }\n```\n'
    )
    result = lint(src)
    _buf, report = write_xlsx(build_workbook_model(result.doc), src.encode("utf-8"))
    assert any("carried via source part" in line and "threads=1" in line for line in report)
    assert any("lossless round-trip" in line for line in report)


def test_cached_helpers_defensive():
    assert _scalar_cell("A1", None, 1900) == ""
    assert _cached_type(None) == ""
    assert _cached_value(None, 1900) == ""
    assert _cached_value(Scalar(kind="blank"), 1900) == ""  # exhausts the fall-through
    assert _cached_type(Scalar(kind="date", value="2026-07-04")) == ""


# ---- carry round-trip ----
def test_carry_restore_is_verbatim():
    src = '---\ngridmd: "0.1"\n---\n# S\n@ A1 "héllo — x"\n'
    buf, _ = _xlsx_from(src)
    restored, report = xlsx_to_gridmd(buf)
    assert restored == src
    assert report[0].action == "restored"


# ---- native import (foreign-style, no carry) ----
def _zip(parts: dict[str, str]) -> bytes:
    return zip_write([(name, data.encode("utf-8")) for name, data in parts.items()])


def test_native_import_reverse_parses_worksheet():
    wb = (
        '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>'
        '<sheet name="Data" sheetId="1" r:id="rId1"/>'
        '<sheet name="Hid" state="veryHidden" sheetId="2" r:id="rId2"/></sheets></workbook>'
    )
    rels = (
        '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="t" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" Type="t" Target="/xl/worksheets/missing.xml"/></Relationships>'
    )
    shared = f'<?xml version="1.0"?><sst {XMLNS_WS}><si><t>shared text</t></si></sst>'
    sheet1 = (
        f'<?xml version="1.0"?><worksheet {XMLNS_WS}><sheetData>'
        '<row r="1">'
        '<c r="A1"><v>5</v></c>'
        '<c r="B1" t="inlineStr"><is><t>hi</t></is></c>'
        '<c r="C1" t="b"><v>1</v></c>'
        '<c r="D1" t="e"><v>#N/A</v></c>'
        '<c r="E1"><f>A1*2</f><v>10</v></c>'
        '<c r="F1" t="s"><v>0</v></c>'
        '<c r="G1" t="str"><v>plain</v></c>'
        '<c r="H1" t="b"><f t="array" ref="H1:H2">SEQUENCE(2)</f><v>1</v></c>'
        # skipped cells: empty formula, no-value variants, bad ref, oob shared idx
        '<c r="I1"><f></f></c>'
        '<c r="J1" t="e"></c>'
        '<c r="K1" t="b"></c>'
        '<c r="L1"></c>'
        '<c r="M1" t="s"></c>'
        '<c r="N1" t="s"><v>99</v></c>'
        '<c r="badref"><v>1</v></c>'
        '</row>'
        '</sheetData><mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells></worksheet>'
    )
    gmd, report = xlsx_to_gridmd(
        _zip(
            {
                "xl/workbook.xml": wb,
                "xl/_rels/workbook.xml.rels": rels,
                "xl/sharedStrings.xml": shared,
                "xl/worksheets/sheet1.xml": sheet1,
            }
        )
    )
    assert lint(gmd).errors == []
    assert '@ A1 5' in gmd and '@ B1 "hi"' in gmd and '@ C1 TRUE' in gmd
    assert '@ E1 =A1*2 :: 10' in gmd
    assert '@ F1 "shared text"' in gmd  # shared-string resolved
    assert '@ A1:B1 { merge: true }' in gmd
    assert "hidden: very" in gmd
    # honest report + missing worksheet part note
    assert any(r.action == "not-emitted" for r in report)
    assert any(r.feature == "worksheet core" for r in report)


def test_native_import_multiline_text():
    sheet1 = (
        f'<?xml version="1.0"?><worksheet {XMLNS_WS}><sheetData>'
        '<row r="1"><c r="A1" t="inlineStr"><is><t>line1\nline2</t></is></c></row>'
        "</sheetData></worksheet>"
    )
    wb = (
        f'<?xml version="1.0"?><workbook {XMLNS_WS} '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>'
    )
    rels = (
        '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="t" Target="worksheets/sheet1.xml"/></Relationships>'
    )
    gmd, _ = xlsx_to_gridmd(_zip({"xl/workbook.xml": wb, "xl/_rels/workbook.xml.rels": rels, "xl/worksheets/sheet1.xml": sheet1}))
    assert 'value: "line1\\nline2"' in gmd
    assert lint(gmd).errors == []


def test_native_import_no_rels_and_no_sheetdata():
    wb = (
        f'<?xml version="1.0"?><workbook {XMLNS_WS} '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="Empty" sheetId="1" r:id="rId9"/></sheets></workbook>'
    )
    gmd, report = xlsx_to_gridmd(_zip({"xl/workbook.xml": wb}))
    assert "# Empty" in gmd
    assert all(r.action != "not-imported" for r in report)  # bare package: nothing extra to report


def test_native_import_missing_workbook():
    with pytest.raises(ValueError, match="missing xl/workbook.xml"):
        xlsx_to_gridmd(_zip({"xl/styles.xml": "<x/>"}))


# ---- carry error paths ----
def test_carry_missing_source():
    buf = _zip({CARRY_PART: '<?xml version="1.0"?><gridmd xmlns="urn:gridmd:carry"></gridmd>'})
    with pytest.raises(ValueError, match="missing <source>"):
        xlsx_to_gridmd(buf)


def test_carry_wrong_encoding():
    buf = _zip({CARRY_PART: '<?xml version="1.0"?><gridmd><source encoding="hex">AA</source></gridmd>'})
    with pytest.raises(ValueError, match="unsupported carry encoding"):
        xlsx_to_gridmd(buf)


def test_carry_corrupt_payload():
    buf = _zip({CARRY_PART: '<?xml version="1.0"?><gridmd><source encoding="base64">!!!notb64!!!</source></gridmd>'})
    with pytest.raises(ValueError, match="corrupt carry payload"):
        xlsx_to_gridmd(buf)


def test_carry_malformed_xml():
    buf = _zip({CARRY_PART: "<gridmd><source>unclosed"})
    with pytest.raises(ValueError, match="malformed carry part"):
        xlsx_to_gridmd(buf)


# ---- XML hardening ----
def test_parse_xml_rejects_dtd():
    with pytest.raises(ValueError, match="DTD/entity"):
        _parse_xml('<!DOCTYPE r [<!ENTITY x "y">]><r/>')


def test_yaml_dq_escapes_all_controls():
    # XML text can't carry raw control chars, so exercise the escaper directly.
    assert _yaml_dq('a"b\\c\td\re\nf\x01g') == '"a\\"b\\\\c\\td\\re\\nf\\x01g"'


def test_main_module_importable():
    import gridmd.__main__ as entry  # covers the module-level imports

    assert entry.main is not None


# ---- zipio ----
def test_zip_round_trip_deterministic():
    a = zip_write([("a.txt", b"hello"), ("b.txt", b"world")])
    b = zip_write([("a.txt", b"hello"), ("b.txt", b"world")])
    assert a == b  # deterministic
    assert zip_read(a) == {"a.txt": b"hello", "b.txt": b"world"}


def test_zip_read_not_a_zip():
    with pytest.raises(ValueError, match="not a valid zip"):
        zip_read(b"definitely not a zip file")


def test_zip_read_corrupt_member():
    buf = bytearray(zip_write([("a.txt", b"hello")]))
    # local header (30) + name "a.txt" (5) → data begins at offset 35
    buf[35] ^= 0xFF
    with pytest.raises(ValueError, match="corrupt zip member"):
        zip_read(bytes(buf))
