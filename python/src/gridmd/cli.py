"""GridMD command-line interface: ``dump`` | ``to-xlsx`` | ``from-xlsx``.

Exit codes: ``0`` success, ``1`` invalid input / operation error. User-facing
problems are printed to stderr as diagnostics, never as tracebacks.
"""

from __future__ import annotations

import argparse
import sys
from typing import TextIO

from . import lint
from .dump import dump_model
from .model import build_workbook_model
from .xlsxread import xlsx_to_gridmd
from .xlsxwrite import write_xlsx


def _read_bytes(path: str) -> bytes:
    with open(path, "rb") as fh:
        return fh.read()


def _print_diagnostics(diags: list, stream: TextIO) -> None:
    for d in diags:
        print(f"error: line {d.line}: {d.msg}", file=stream)


def _cmd_dump(path: str, stdout: TextIO, stderr: TextIO) -> int:
    source = _read_bytes(path).decode("utf-8")
    result = lint(source)
    if result.errors:
        _print_diagnostics(result.errors, stderr)
        return 1
    stdout.write(dump_model(build_workbook_model(result.doc)))
    return 0


def _cmd_to_xlsx(path: str, output: str, stderr: TextIO) -> int:
    source = _read_bytes(path)
    result = lint(source.decode("utf-8"))
    if result.errors:
        _print_diagnostics(result.errors, stderr)
        return 1
    buffer, report = write_xlsx(build_workbook_model(result.doc), source)
    with open(output, "wb") as fh:
        fh.write(buffer)
    print(f"wrote {output} ({len(buffer)} bytes)", file=stderr)
    for line in report:
        print(f"  {line}", file=stderr)
    return 0


def _cmd_from_xlsx(path: str, output: str, stderr: TextIO) -> int:
    gmd, report = xlsx_to_gridmd(_read_bytes(path))
    with open(output, "w", encoding="utf-8") as fh:
        fh.write(gmd)
    # Self-check: the emitted GridMD must itself pass strict lint.
    result = lint(gmd)
    if result.errors:
        print("error: imported GridMD failed strict lint (self-check):", file=stderr)
        _print_diagnostics(result.errors, stderr)
        return 1
    print(f"wrote {output} ({len(gmd)} bytes)", file=stderr)
    for entry in report:
        note = f" — {entry.note}" if entry.note else ""
        print(f"  {entry.feature}: {entry.action}{note}", file=stderr)
    return 0


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="gridmd", description="GridMD ⇄ XLSX converter and canonical dumper.")
    sub = parser.add_subparsers(dest="command", required=True)

    p_dump = sub.add_parser("dump", help="canonical model dump to stdout")
    p_dump.add_argument("file", help="GridMD (.gmd) source file")

    p_to = sub.add_parser("to-xlsx", help="export a .gmd to .xlsx")
    p_to.add_argument("file", help="GridMD (.gmd) source file")
    p_to.add_argument("-o", "--output", required=True, help="output .xlsx path")

    p_from = sub.add_parser("from-xlsx", help="import a .xlsx back to .gmd")
    p_from.add_argument("file", help="input .xlsx package")
    p_from.add_argument("-o", "--output", required=True, help="output .gmd path")
    return parser


def main(argv: list[str] | None = None, stdout: TextIO | None = None, stderr: TextIO | None = None) -> int:
    stdout = stdout if stdout is not None else sys.stdout
    stderr = stderr if stderr is not None else sys.stderr
    args = _build_parser().parse_args(argv)
    try:
        if args.command == "dump":
            return _cmd_dump(args.file, stdout, stderr)
        if args.command == "to-xlsx":
            return _cmd_to_xlsx(args.file, args.output, stderr)
        return _cmd_from_xlsx(args.file, args.output, stderr)
    except (OSError, ValueError, UnicodeDecodeError) as e:
        print(f"error: {e}", file=stderr)
        return 1
