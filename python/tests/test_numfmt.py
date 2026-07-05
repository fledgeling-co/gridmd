"""ECMAScript Number→String formatting (numfmt)."""

from __future__ import annotations

import math

import pytest

from gridmd.numfmt import format_number


@pytest.mark.parametrize(
    "value,expected",
    [
        (0, "0"),
        (-0.0, "0"),
        (1000, "1000"),
        (1000.0, "1000"),
        (0.3, "0.3"),
        (-12.5, "-12.5"),
        (987.8, "987.8"),
        (100, "100"),
        (0.6, "0.6"),
        (0.3771428571428571, "0.3771428571428571"),
        (0.0001, "0.0001"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (1e21, "1e+21"),
        (1.5e-8, "1.5e-8"),
        (123.45, "123.45"),
        (2003.05, "2003.05"),
        (234, "234"),
        (99.5, "99.5"),
    ],
)
def test_format_number(value, expected):
    assert format_number(value) == expected


def test_non_finite():
    assert format_number(math.nan) == "NaN"
    assert format_number(math.inf) == "Infinity"
    assert format_number(-math.inf) == "-Infinity"
