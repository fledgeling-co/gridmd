"""GridMD — a plain-text spreadsheet format with a two-way XLSX converter.

Public API surface. ``lint`` parses + validates; ``dump`` renders the canonical
model dump; the XLSX seam is :func:`write_xlsx` / :func:`xlsx_to_gridmd`.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from .dump import dump_model
from .model import WorkbookModel, build_workbook_model, translate_formula
from .parser import Diagnostic, ParsedDocument, parse_document
from .refs import parse_cell, parse_target
from .scalar import parse_scalar, split_cached
from .validate import validate_document
from .xlsxread import ImportReport, xlsx_to_gridmd
from .xlsxwrite import write_xlsx

__version__ = "0.1.0"

__all__ = [
    "lint",
    "dump",
    "LintResult",
    "GridmdError",
    "parse_document",
    "validate_document",
    "build_workbook_model",
    "dump_model",
    "write_xlsx",
    "xlsx_to_gridmd",
    "translate_formula",
    "parse_scalar",
    "split_cached",
    "parse_cell",
    "parse_target",
    "Diagnostic",
    "ImportReport",
    "WorkbookModel",
    "ParsedDocument",
    "__version__",
]


@dataclass
class LintResult:
    doc: ParsedDocument
    errors: list[Diagnostic] = field(default_factory=list)
    warnings: list[Diagnostic] = field(default_factory=list)


class GridmdError(Exception):
    """Raised when a document fails strict-mode validation."""

    def __init__(self, errors: list[Diagnostic]):
        self.errors = errors
        super().__init__("; ".join(f"line {e.line}: {e.msg}" for e in errors))


def lint(source: str, mode: str = "strict") -> LintResult:
    """Parse + validate ``source``; return the document and line-sorted diagnostics."""
    doc = parse_document(source, mode)
    validate_document(doc)
    by_line = lambda d: d.line
    return LintResult(doc, sorted(doc.errors, key=by_line), sorted(doc.warnings, key=by_line))


def dump(source: str, mode: str = "strict") -> str:
    """Canonical model dump of ``source``; raises :class:`GridmdError` if invalid."""
    result = lint(source, mode)
    if result.errors:
        raise GridmdError(result.errors)
    return dump_model(build_workbook_model(result.doc))
