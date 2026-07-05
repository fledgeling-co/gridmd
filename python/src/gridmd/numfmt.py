"""ECMAScript ``Number → String`` formatting.

The canonical dump (``conformance/README.md``) renders IEEE-754 doubles exactly
as JavaScript's ``String(n)`` does: shortest round-trip decimal, integer-valued
doubles with no decimal point (``1000`` not ``1e3`` not ``1000.0``), fixed
notation inside the standard range and exponential outside it. Python's
``repr``/``str`` disagree on both integers (``1000.0``) and exponent placement
(``1e+21``/``1e-07``), so this is hand-rolled to match V8 exactly.
"""

from __future__ import annotations

import math
from decimal import Decimal


def format_number(value: float | int) -> str:
    """Render ``value`` using ECMAScript ``Number::toString`` semantics."""
    f = float(value)
    if f == 0:
        return "0"  # covers +0 and -0
    if math.isnan(f):
        return "NaN"
    if math.isinf(f):
        return "Infinity" if f > 0 else "-Infinity"

    sign = ""
    if f < 0:
        sign = "-"
        f = -f

    # Shortest round-trip significant digits + decimal exponent.  repr() gives
    # the shortest decimal that round-trips; Decimal(...).normalize() strips
    # trailing zeros so the remaining digit tuple is exactly the significand.
    digits_tuple, exp = _significand(f)
    digits = "".join(str(d) for d in digits_tuple)
    k = len(digits)  # significant digit count
    n = k + exp  # ES §Number::toString decimal-point position

    if k <= n <= 21:
        return sign + digits + "0" * (n - k)
    if 0 < n <= 21:
        return sign + digits[:n] + "." + digits[n:]
    if -6 < n <= 0:
        return sign + "0." + "0" * (-n) + digits
    return sign + _exponential(digits, k, n)


def _significand(f: float) -> tuple[tuple[int, ...], int]:
    """Return ``(digit_tuple, exp)`` such that ``value == int(digits) * 10**exp``
    with no trailing zero digits."""
    _, digits_tuple, exp = Decimal(repr(f)).normalize().as_tuple()
    return digits_tuple, exp


def _exponential(digits: str, k: int, n: int) -> str:
    head = digits[0]
    if k > 1:
        head += "." + digits[1:]
    e = n - 1
    return head + ("e+" + str(e) if e >= 0 else "e-" + str(-e))
