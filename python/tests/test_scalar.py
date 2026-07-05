"""Cell scalar micro-grammar (scalar)."""

from __future__ import annotations

from gridmd.scalar import parse_scalar, split_cached


def test_split_cached_none():
    assert split_cached("=A1*2") == ("=A1*2", None)


def test_split_cached_last():
    assert split_cached("x :: y :: z") == ("x :: y", "z")


def test_split_cached_ignores_quoted():
    assert split_cached('="a :: b"') == ('="a :: b"', None)


def test_blank():
    assert parse_scalar("").kind == "blank"


def test_number():
    for raw, val in [("42", 42.0), ("1e3", 1000.0), ("-12.5", -12.5), ("0.3", 0.3)]:
        s = parse_scalar(raw)
        assert s.kind == "number" and s.value == val


def test_boolean():
    assert parse_scalar("TRUE").value is True
    assert parse_scalar("false").value is False


def test_dates_and_times():
    assert parse_scalar("2026-07-04").kind == "date"
    assert parse_scalar("2026-07-04T06:00").kind == "date"
    assert parse_scalar("12:30").kind == "time"
    assert parse_scalar("12:30:45").kind == "time"


def test_errors():
    assert parse_scalar("#DIV/0!").kind == "error"
    assert parse_scalar("#n/a").value == "#N/A"


def test_forced_text():
    s = parse_scalar("'TRUE")
    assert s.kind == "text" and s.value == "TRUE" and s.forced


def test_quoted_text():
    s = parse_scalar('"a ""b"" c"')
    assert s.kind == "text" and s.value == 'a "b" c' and s.quoted


def test_unterminated_quote():
    s = parse_scalar('"oops')
    assert s.kind == "text" and s.problem == "unterminated quoted text"


def test_plain_text():
    assert parse_scalar("hello world").kind == "text"


def test_formula_with_cached():
    s = parse_scalar("=A1*2 :: 3")
    assert s.kind == "formula" and s.formula == "A1*2"
    assert s.cached.kind == "number" and s.cached.value == 3.0


def test_cse_formula():
    s = parse_scalar("{=SUM(A1:A3)} :: 6")
    assert s.kind == "formula" and s.cse and s.formula == "SUM(A1:A3)"
    assert s.cached.value == 6.0


def test_cse_unterminated():
    s = parse_scalar("{=SUM(A1:A3)")
    assert s.kind == "text" and s.problem == "unterminated CSE array formula"


def test_cached_may_not_be_formula():
    s = parse_scalar("=A1 :: =B1")
    assert s.cached.kind == "invalid" and s.cached.problem == "cached value must not be a formula"
