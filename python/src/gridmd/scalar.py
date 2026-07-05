"""Cell scalar micro-grammar (SPEC.md §6)."""

from __future__ import annotations

import re
from dataclasses import dataclass

ERROR_VALUES = frozenset(
    [
        "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A",
        "#GETTING_DATA", "#SPILL!", "#CALC!", "#FIELD!", "#BLOCKED!",
    ]
)

_NUMBER_RE = re.compile(r"^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?$")
_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?)?$")
_TIME_RE = re.compile(r"^\d{2}:\d{2}(:\d{2})?$")
_DQ_RE = re.compile(r'^"((?:[^"]|"")*)"$')


@dataclass
class Scalar:
    kind: str  # blank|text|formula|number|boolean|date|time|error|invalid
    value: str | float | bool | None = None
    formula: str | None = None
    cse: bool = False
    cached: "Scalar | None" = None
    problem: str | None = None
    forced: bool = False
    quoted: bool = False


def split_cached(text: str) -> tuple[str, str | None]:
    """Split ``"formula :: cached"`` at the LAST ``" :: "`` outside double-quoted
    string literals (SPEC §6). Returns ``(head, cached)`` (cached ``None`` if none)."""
    in_q = False
    idx = -1
    for i, ch in enumerate(text):
        if ch == '"':
            in_q = not in_q
            continue
        if not in_q and ch == " " and text.startswith(" :: ", i):
            idx = i
    if idx == -1:
        return text, None
    return text[:idx], text[idx + 4 :].strip()


def parse_scalar(raw: str) -> Scalar:
    """Parse one cell scalar. ``raw`` must already be trimmed."""
    if raw == "":
        return Scalar(kind="blank")

    if raw.startswith("{="):
        head, cached = split_cached(raw)
        if not head.endswith("}"):
            return Scalar(kind="text", value=raw, problem="unterminated CSE array formula")
        return Scalar(kind="formula", cse=True, formula=head[2:-1], cached=_parse_cached(cached))
    if raw.startswith("="):
        head, cached = split_cached(raw)
        return Scalar(kind="formula", cse=False, formula=head[1:], cached=_parse_cached(cached))
    if raw.startswith("'"):
        return Scalar(kind="text", value=raw[1:], forced=True)
    if raw.startswith('"'):
        m = _DQ_RE.match(raw)
        if m:
            return Scalar(kind="text", value=m.group(1).replace('""', '"'), quoted=True)
        return Scalar(kind="text", value=raw, problem="unterminated quoted text")
    if _NUMBER_RE.match(raw):
        return Scalar(kind="number", value=float(raw))
    up = raw.upper()
    if up in ("TRUE", "FALSE"):
        return Scalar(kind="boolean", value=up == "TRUE")
    if _DATE_RE.match(raw):
        return Scalar(kind="date", value=raw)
    if _TIME_RE.match(raw):
        return Scalar(kind="time", value=raw)
    if up in ERROR_VALUES:
        return Scalar(kind="error", value=up)
    return Scalar(kind="text", value=raw)


def _parse_cached(cached_text: str | None) -> Scalar | None:
    if cached_text is None:
        return None
    v = parse_scalar(cached_text)
    if v.kind == "formula":
        return Scalar(kind="invalid", problem="cached value must not be a formula")
    return v
