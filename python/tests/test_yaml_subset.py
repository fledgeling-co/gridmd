"""The GridMD YAML safe subset (parser.parse_yaml / try_props)."""

from __future__ import annotations

from gridmd.parser import Diagnostic, parse_yaml, try_props


def _yaml(text):
    errors: list[Diagnostic] = []
    value = parse_yaml(text, 1, errors)
    return value, errors


def test_empty_is_empty_map():
    value, errors = _yaml("   ")
    assert value == {} and errors == []


def test_true_false_are_bool():
    value, errors = _yaml("a: true\nb: false\nc: TRUE")
    assert value == {"a": True, "b": False, "c": True} and errors == []


def test_yes_no_on_off_are_strings():
    value, _ = _yaml("a: yes\nb: no\nc: on\nd: off\ne: Y")
    assert value == {"a": "yes", "b": "no", "c": "on", "d": "off", "e": "Y"}


def test_sexagesimal_stays_string():
    value, _ = _yaml("t: 12:30\nu: 1:2:3")
    assert value == {"t": "12:30", "u": "1:2:3"}


def test_timestamp_stays_string():
    value, _ = _yaml("at: 2026-07-01T10:00:00Z\nd: 2026-07-04")
    assert value["at"] == "2026-07-01T10:00:00Z"
    assert value["d"] == "2026-07-04" and isinstance(value["d"], str)


def test_numbers():
    value, _ = _yaml("i: 42\nf: 1e3\ng: 1.5\nh: 0x1F\no: 0o17\nneg: -3")
    assert value == {"i": 42, "f": 1000.0, "g": 1.5, "h": 31, "o": 15, "neg": -3}


def test_null_forms():
    value, _ = _yaml("a: null\nb: ~\nc:\nd: NULL")
    assert value == {"a": None, "b": None, "c": None, "d": None}


def test_reject_anchor_alias():
    _value, errors = _yaml("a: &x 1\nb: *x")
    assert any("anchors/aliases" in e.msg for e in errors)


def test_reject_explicit_tag():
    _value, errors = _yaml('a: !!str 5')
    assert any("tags" in e.msg for e in errors)


def test_reject_duplicate_keys():
    _value, errors = _yaml("a: 1\na: 2")
    assert any("YAML" in e.msg for e in errors)


def test_malformed_yaml():
    _value, errors = _yaml("a: [1, 2")
    assert errors and errors[0].msg.startswith("YAML:")


def test_try_props_valid():
    assert try_props("{ merge: true, style: hdr }") == {"merge": True, "style": "hdr"}


def test_try_props_non_mapping():
    assert try_props("[1, 2, 3]") is None


def test_try_props_bad_key():
    assert try_props("{ Bad_Key: 1 }") is None


def test_try_props_null_value():
    assert try_props("{ a: null }") is None


def test_try_props_parse_error():
    assert try_props("{ a: [ }") is None


def test_try_props_rejects_anchor():
    assert try_props("{ a: &x 1 }") is None
