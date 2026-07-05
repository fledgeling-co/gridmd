"""Shared fixtures: locate the conformance corpus relative to the repo root."""

from __future__ import annotations

from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
CONFORMANCE = REPO_ROOT / "conformance"
FIXTURES = CONFORMANCE / "fixtures"
EXPECTED = CONFORMANCE / "expected"
INVALID = CONFORMANCE / "invalid"
FIXTURES_XLSX = CONFORMANCE / "fixtures-xlsx"
EXAMPLES = REPO_ROOT / "examples"

# name -> (.gmd source path, expected dump path)
VALID_CASES = {
    "01-cells": (FIXTURES / "01-cells.gmd", EXPECTED / "01-cells.json"),
    "02-structure": (FIXTURES / "02-structure.gmd", EXPECTED / "02-structure.json"),
    "03-features": (FIXTURES / "03-features.gmd", EXPECTED / "03-features.json"),
    "quarterly-report": (EXAMPLES / "quarterly-report.gmd", EXPECTED / "quarterly-report.json"),
}


@pytest.fixture(params=sorted(VALID_CASES), ids=sorted(VALID_CASES))
def valid_case(request):
    src, expected = VALID_CASES[request.param]
    return request.param, src, expected
