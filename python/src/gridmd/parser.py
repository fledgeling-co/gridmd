"""GridMD document parser (SPEC.md §2-§10, Appendix A).

Produces a block tree; semantic checks live in :mod:`gridmd.validate`.

The embedded YAML is restricted to the GridMD safe subset: no anchors, aliases
or explicit tags, and the YAML-1.2-core scalar schema (only ``true``/``false``
are booleans — ``yes``/``no``/``on``/``off`` and sexagesimal ``12:30`` stay
plain strings; ISO timestamps stay strings, never Python ``datetime``).
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any

import yaml

RESERVED_KINDS = frozenset(
    [
        "sheet", "grid", "spill-cache", "table", "cf", "validation", "filter",
        "chart", "sparklines", "pivot", "slicer", "image", "shape", "textbox",
        "checkbox", "comments", "outline", "page", "query", "script", "scenario",
        "raw",
    ]
)

_IDENT_KEY_RE = re.compile(r"^(x-)?[a-z][a-z0-9-]*$")
_FENCE_OPEN_RE = re.compile(r"^(`{3,})\{([A-Za-z][A-Za-z0-9-]*)\}(.*)$")


# ---- diagnostics + block tree ----
@dataclass
class Diagnostic:
    line: int
    msg: str


@dataclass
class InfoArgs:
    positional: list[str] = field(default_factory=list)
    flags: dict[str, str] = field(default_factory=dict)
    anchor: str | None = None
    size: dict[str, int] | None = None


@dataclass
class Row:
    cells: list[str]
    line: int


@dataclass
class FenceBlock:
    kind: str
    args: InfoArgs
    body: list[str]
    line: int
    type: str = "fence"
    meta: Any = None
    rows: list[Row] | None = None
    code: str | None = None
    payload: str | None = None


@dataclass
class AtBlock:
    target_text: str
    line: int
    type: str = "at"
    scalar_text: str | None = None
    props: Any = None
    body: Any = None


@dataclass
class SheetBlock:
    name: str
    line: int
    blocks: list = field(default_factory=list)


@dataclass
class ParseStats:
    defs: int = 0
    blocks: int = 0


@dataclass
class ParsedDocument:
    frontmatter: Any = field(default_factory=dict)
    workbook_blocks: list = field(default_factory=list)
    sheets: list[SheetBlock] = field(default_factory=list)
    errors: list[Diagnostic] = field(default_factory=list)
    warnings: list[Diagnostic] = field(default_factory=list)
    mode: str = "strict"
    stats: ParseStats | None = None


# ---- YAML safe-subset loader ----
class _GridmdSafeLoader(yaml.SafeLoader):
    """SafeLoader restricted to the GridMD YAML subset with duplicate-key
    detection."""

    def construct_mapping(self, node: Any, deep: bool = False) -> dict:
        # PyYAML only dispatches here for mapping nodes (the map-tag constructor),
        # so no node-kind guard is needed.
        mapping: dict = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in mapping:
                raise yaml.constructor.ConstructorError("while constructing a mapping", node.start_mark, f"found duplicate key {key!r}", key_node.start_mark)
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


# Replace YAML-1.1 implicit resolvers with the YAML-1.2 core scalar schema.
_GridmdSafeLoader.yaml_implicit_resolvers = {}
_GridmdSafeLoader.add_implicit_resolver(
    "tag:yaml.org,2002:bool",
    re.compile(r"^(?:true|True|TRUE|false|False|FALSE)$"),
    list("tTfF"),
)
_GridmdSafeLoader.add_implicit_resolver(
    "tag:yaml.org,2002:int",
    re.compile(r"^(?:[-+]?[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+)$"),
    list("-+0123456789"),
)
_GridmdSafeLoader.add_implicit_resolver(
    "tag:yaml.org,2002:float",
    re.compile(
        r"^(?:[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?"
        r"|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$"
    ),
    list("-+0123456789."),
)
_GridmdSafeLoader.add_implicit_resolver(
    "tag:yaml.org,2002:null",
    re.compile(r"^(?:~|null|Null|NULL|)$"),
    ["~", "n", "N", ""],
)


def _reject_unsafe(text: str) -> str | None:
    """Return an error message if the YAML uses anchors, aliases or tags.

    Both an anchor definition and an alias reference carry ``.anchor`` on their
    parse event (the definition always precedes any use in a valid document), so
    a single anchor check rejects both forms.
    """
    for event in yaml.parse(text, Loader=_GridmdSafeLoader):
        if getattr(event, "anchor", None) is not None:
            return "YAML anchors/aliases are outside the GridMD safe subset"
        if getattr(event, "tag", None) is not None:
            return "YAML tags are outside the GridMD safe subset"
    return None


def _load_yaml(text: str) -> Any:
    return yaml.load(text, Loader=_GridmdSafeLoader)


def parse_yaml(text: str, line: int, errors: list[Diagnostic]) -> Any:
    if text.strip() == "":
        return {}
    try:
        unsafe = _reject_unsafe(text)
        if unsafe:
            errors.append(Diagnostic(line, unsafe))
        value = _load_yaml(text)
    except yaml.YAMLError as e:  # malformed YAML
        first = str(e).split("\n")[0]
        errors.append(Diagnostic(line, f"YAML: {first}"))
        return {}
    return {} if value is None else value


def try_props(text: str) -> Any:
    """YAML flow-map candidate for @-directive props: parses to a mapping in
    which every top-level key is an identifier and every value is non-null."""
    try:
        if _reject_unsafe(text) is not None:
            return None
        v = _load_yaml(text)
    except yaml.YAMLError:
        return None
    if v is None or not isinstance(v, dict):
        return None
    for k, val in v.items():
        if not isinstance(k, str) or not _IDENT_KEY_RE.match(k):
            return None
        if val is None:
            return None
    return v


def find_props_split(text: str) -> tuple[str, str | None]:
    """Right-edge props split (SPEC §9.1 / Appendix A): the last brace-balanced
    ``{...}`` group (double quotes respected) that runs to end-of-line and is
    preceded by whitespace."""
    if not text.endswith("}"):
        return text, None
    in_q = False
    depth = 0
    start = -1
    last_group: tuple[int, int] | None = None
    for i, ch in enumerate(text):
        if ch == '"':
            in_q = not in_q
            continue
        if in_q:
            continue
        if ch == "{":
            if depth == 0:
                start = i
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0 and start != -1:
                last_group = (start, i)
            if depth < 0:
                return text, None
    if not last_group:
        return text, None
    s, e = last_group
    if e != len(text) - 1 or s == 0 or text[s - 1] != " ":
        return text, None
    return text[:s].rstrip(), text[s:]


def split_pipe_row(raw_line: str) -> list[str] | None:
    """Pipe row → trimmed cell strings; backslash escapes the next character.
    Returns ``None`` if the line is not a well-formed pipe row."""
    line = re.sub(r"\s+$", "", raw_line)
    if not line.startswith("|") or len(line) < 2:
        return None
    cells: list[str] = []
    cell = ""
    opened = False
    i = 0
    while i < len(line):
        ch = line[i]
        if ch == "\\" and i + 1 < len(line):
            cell += line[i + 1]
            i += 2
            continue
        if ch == "|":
            if not opened:
                opened = True
                i += 1
                continue
            cells.append(cell.strip())
            cell = ""
            i += 1
            continue
        cell += ch
        i += 1
    if cell.strip() != "":
        return None  # no unescaped closing pipe
    return cells


_INFO_TOKEN_RE = re.compile(r'"((?:[^"]|"")*)"|\S+')
_FLAG_RE = re.compile(r"^([A-Za-z][A-Za-z0-9-]*)=(.*)$", re.DOTALL)
_SIZE_RE = re.compile(r"^(\d+)x(\d+)$")


def parse_info_args(rest: str, line: int, errors: list[Diagnostic]) -> InfoArgs:
    """Fence info string: positional args (quoted ``""`` doubling), at-anchors,
    size WxH, key=val flags."""
    out = InfoArgs()
    tokens: list[tuple[str, bool]] = []
    for m in _INFO_TOKEN_RE.finditer(rest):
        if m.group(1) is not None:
            tokens.append((m.group(1).replace('""', '"'), True))
        else:
            tokens.append((m.group(0), False))
    k = 0
    while k < len(tokens):
        value, quoted = tokens[k]
        if not quoted and value == "at":
            k += 1
            if k >= len(tokens):
                errors.append(Diagnostic(line, "`at` requires an anchor"))
                break
            out.anchor = tokens[k][0]
            k += 1
            continue
        if not quoted and value == "size":
            k += 1
            sm = _SIZE_RE.match(tokens[k][0]) if k < len(tokens) else None
            if not sm:
                errors.append(Diagnostic(line, "`size` requires WxH (e.g. 480x320)"))
                k += 1
                continue
            out.size = {"w": int(sm.group(1)), "h": int(sm.group(2))}
            k += 1
            continue
        fm = _FLAG_RE.match(value) if not quoted else None
        if fm:
            flag_val = re.sub(r'^"(.*)"$', r"\1", fm.group(2), flags=re.DOTALL).replace('""', '"')
            out.flags[fm.group(1)] = flag_val
            k += 1
            continue
        out.positional.append(value)
        k += 1
    return out


def _meta(arr: list[str], line: int, off: int, errors: list[Diagnostic]) -> Any:
    return parse_yaml("\n".join(arr), line + off, errors)


def _refine_fence(block: FenceBlock, errors: list[Diagnostic]) -> None:
    kind, body, line = block.kind, block.body, block.line
    if kind in ("grid", "spill-cache"):
        block.rows = _parse_rows(body, line, errors)
    elif kind == "table":
        d = body.index("---") if "---" in body else -1
        if d == -1:
            errors.append(Diagnostic(line, "{table} requires a `---`-separated payload of pipe rows"))
            block.meta = _meta(body, line, 1, errors)
            block.rows = []
        else:
            block.meta = _meta(body[:d], line, 1, errors)
            block.rows = _parse_rows(body[d + 1 :], line + d + 1, errors)
    elif kind == "script":
        d = body.index("---") if "---" in body else -1
        if d == -1:
            block.meta = {}
            block.code = "\n".join(body)
        else:
            block.meta = _meta(body[:d], line, 1, errors)
            block.code = "\n".join(body[d + 1 :])
    elif kind == "raw" or kind.startswith("x-"):
        block.payload = "\n".join(body)
    else:
        block.meta = _meta(body, line, 1, errors)


def _parse_rows(body_lines: list[str], base_line: int, errors: list[Diagnostic]) -> list[Row]:
    rows: list[Row] = []
    for k, line_text in enumerate(body_lines):
        if line_text.strip() == "":
            continue
        cells = split_pipe_row(line_text)
        if cells is None:
            errors.append(Diagnostic(base_line + k + 1, f"expected a pipe row, got: {line_text[:50]}"))
            continue
        rows.append(Row(cells=cells, line=base_line + k + 1))
    return rows


def _parse_fence(lines: list[str], i: int, m: re.Match, errors: list[Diagnostic]) -> tuple[FenceBlock, int]:
    open_len = len(m.group(1))
    kind = m.group(2)
    args = parse_info_args(m.group(3) or "", i + 1, errors)
    body: list[str] = []
    j = i + 1
    closed = False
    close_re = re.compile(rf"^`{{{open_len},}}\s*$")
    while j < len(lines):
        if close_re.match(lines[j]):
            closed = True
            j += 1
            break
        body.append(lines[j])
        j += 1
    if not closed:
        errors.append(Diagnostic(i + 1, f"unclosed {{{kind}}} fence"))
    block = FenceBlock(kind=kind, args=args, body=body, line=i + 1)
    _refine_fence(block, errors)
    return block, j


def _parse_at(lines: list[str], i: int, errors: list[Diagnostic]) -> tuple[AtBlock, int]:
    line = lines[i]
    rest = line[2:]
    sp = rest.find(" ")
    target_text = rest if sp == -1 else rest[:sp]
    inline = "" if sp == -1 else rest[sp + 1 :].strip()

    # Multiline body: maximal run of blank-or-2-space-indented lines, trailing
    # blanks excluded (Appendix A, dedent rule).
    j = i + 1
    taken = 0
    last_take = 0
    acc: list[str] = []
    while j < len(lines):
        current = lines[j]
        if current.strip() == "":
            acc.append("")
            j += 1
            taken += 1
            continue
        if re.match(r"^ {2}", current):
            acc.append(current[2:])
            j += 1
            taken += 1
            last_take = taken
            continue
        break
    body_lines = acc[:last_take] if last_take > 0 else None
    nxt = i + 1 + last_take

    block = AtBlock(target_text=target_text, line=i + 1)
    if body_lines is not None:
        parsed = parse_yaml("\n".join(body_lines), i + 2, errors)
        if parsed is None or not isinstance(parsed, dict):
            errors.append(Diagnostic(i + 2, "@ directive body must be a YAML mapping"))
        else:
            block.body = parsed
    if inline != "":
        if inline.startswith("{") and not inline.startswith("{="):
            props = try_props(inline)
            if props:
                block.props = props
                return block, nxt
        scalar_text, props_text = find_props_split(inline)
        if props_text:
            props = try_props(props_text)
            if props:
                block.props = props
                block.scalar_text = None if scalar_text == "" else scalar_text
                return block, nxt
        block.scalar_text = inline
    return block, nxt


def parse_document(source: str, mode: str = "strict") -> ParsedDocument:
    errors: list[Diagnostic] = []
    warnings: list[Diagnostic] = []
    lines = re.split(r"\r?\n", source)
    doc = ParsedDocument(errors=errors, warnings=warnings, mode=mode)

    if not lines or lines[0] != "---":
        errors.append(Diagnostic(1, "document must begin with `---` YAML frontmatter"))
        return doc
    fm_end = -1
    for k in range(1, len(lines)):
        if lines[k] == "---":
            fm_end = k
            break
    if fm_end == -1:
        errors.append(Diagnostic(1, "unterminated frontmatter (missing closing `---`)"))
        return doc
    doc.frontmatter = parse_yaml("\n".join(lines[1:fm_end]), 2, errors)

    i = fm_end + 1
    cur: SheetBlock | None = None

    def push(b: Any) -> None:
        (cur.blocks if cur else doc.workbook_blocks).append(b)

    while i < len(lines):
        line = lines[i]
        if line.strip() == "" or re.match(r"^>", line) or re.match(r"^#{2,}(\s|$)", line):
            i += 1
            continue
        m = re.match(r"^# (.+)$", line)
        if m:
            cur = SheetBlock(name=m.group(1).strip(), line=i + 1)
            doc.sheets.append(cur)
            i += 1
            continue
        m = _FENCE_OPEN_RE.match(line)
        if m:
            block, nxt = _parse_fence(lines, i, m, errors)
            push(block)
            i = nxt
            continue
        if re.match(r"^@ ", line):
            block, nxt = _parse_at(lines, i, errors)
            push(block)
            i = nxt
            continue
        target = errors if mode == "strict" else warnings
        target.append(Diagnostic(i + 1, f"unrecognized line: {line[:60]}"))
        i += 1
    return doc
