"""Document parser: fences, @ directives, props split, pipe rows, info args."""

from __future__ import annotations

from gridmd.parser import (
    Diagnostic,
    find_props_split,
    parse_document,
    parse_info_args,
    split_pipe_row,
)


def _errs(doc):
    return [e.msg for e in doc.errors]


# ---- find_props_split ----
def test_props_split_none_when_no_brace():
    assert find_props_split('"just text"') == ('"just text"', None)


def test_props_split_basic():
    assert find_props_split('42 { numfmt: "0" }') == ("42", '{ numfmt: "0" }')


def test_props_split_unbalanced_close():
    assert find_props_split("a } b {c}") == ("a } b {c}", None)


def test_props_split_group_not_at_eol():
    assert find_props_split("{a} tail") == ("{a} tail", None)


def test_props_split_group_at_start():
    assert find_props_split("{a}") == ("{a}", None)


def test_props_split_needs_leading_space():
    assert find_props_split("x{a}") == ("x{a}", None)


def test_props_split_quotes_protect_braces():
    assert find_props_split('"a}b" {x: 1}') == ('"a}b"', "{x: 1}")


def test_props_split_no_top_group():
    assert find_props_split('"}"') == ('"}"', None)


def test_props_split_never_balances_to_zero():
    # ends with '}' but the braces never close a complete top-level group
    assert find_props_split("{{}") == ("{{}", None)


# ---- split_pipe_row ----
def test_pipe_row_basic():
    assert split_pipe_row("| a | b |") == ["a", "b"]


def test_pipe_row_not_a_row():
    assert split_pipe_row("no pipes") is None


def test_pipe_row_single_bar():
    assert split_pipe_row("|") is None


def test_pipe_row_unterminated():
    assert split_pipe_row("| a | b") is None


def test_pipe_row_escape():
    assert split_pipe_row("| a\\|b | c |") == ["a|b", "c"]


# ---- parse_info_args ----
def _args(rest):
    errors: list[Diagnostic] = []
    return parse_info_args(rest, 1, errors), errors


def test_info_positional_and_quoted():
    args, errors = _args('column "Qty by item"')
    assert args.positional == ["column", "Qty by item"] and errors == []


def test_info_at_anchor():
    args, _ = _args("at H6:M18")
    assert args.anchor == "H6:M18"


def test_info_at_missing():
    _args_, errors = _args("at")
    assert any("`at` requires" in e.msg for e in errors)


def test_info_size():
    args, _ = _args("size 150x200")
    assert args.size == {"w": 150, "h": 200}


def test_info_size_invalid():
    _args_, errors = _args("size wide")
    assert any("`size` requires" in e.msg for e in errors)


def test_info_flags():
    # \S+ tokenization: quoted flag values may not contain spaces; surrounding
    # quotes are stripped and doubled "" collapse to a single ".
    args, _ = _args('lang=js title="Q3" q="a""b"')
    assert args.flags == {"lang": "js", "title": "Q3", "q": 'a"b'}


# ---- parse_document structure ----
def test_missing_frontmatter():
    doc = parse_document("# Sheet\n")
    assert "document must begin with `---` YAML frontmatter" in _errs(doc)


def test_unterminated_frontmatter():
    doc = parse_document("---\ngridmd: \"1.0\"\n")
    assert any("unterminated frontmatter" in m for m in _errs(doc))


def test_sheets_fences_comments_and_headings():
    src = (
        "---\ngridmd: \"1.0\"\n---\n"
        "> a note comment\n"
        "## a sub heading\n"
        "# Main\n"
        "@ A1 1\n"
        "```{grid} B1\n| 2 |\n```\n"
    )
    doc = parse_document(src)
    assert len(doc.sheets) == 1
    assert doc.sheets[0].name == "Main"
    assert len(doc.sheets[0].blocks) == 2


def test_unrecognized_line_strict_is_error():
    doc = parse_document("---\ngridmd: \"1.0\"\n---\n# S\nbogus line\n")
    assert any("unrecognized line" in m for m in _errs(doc))


def test_unrecognized_line_loose_is_warning():
    doc = parse_document("---\ngridmd: \"1.0\"\n---\n# S\nbogus line\n", mode="loose")
    assert doc.errors == []
    assert any("unrecognized line" in w.msg for w in doc.warnings)


def test_workbook_block_before_sheet():
    src = "---\ngridmd: \"1.0\"\n---\n```{query} Q\nsource: { url: x }\n```\n# S\n"
    doc = parse_document(src)
    assert len(doc.workbook_blocks) == 1


def test_unclosed_fence():
    doc = parse_document("---\ngridmd: \"1.0\"\n---\n# S\n```{grid} A1\n| 1 |\n")
    assert any("unclosed" in m for m in _errs(doc))


def test_table_requires_separator():
    src = "---\ngridmd: \"1.0\"\n---\n# S\n```{table} T at A1\n| a |\n| 1 |\n```\n"
    doc = parse_document(src)
    assert any("requires a `---`-separated" in m for m in _errs(doc))


def test_table_with_separator():
    src = "---\ngridmd: \"1.0\"\n---\n# S\n```{table} T at A1\nstyle: light-1\n---\n| a |\n| 1 |\n```\n"
    doc = parse_document(src)
    fence = doc.sheets[0].blocks[0]
    assert fence.meta == {"style": "light-1"} and len(fence.rows) == 2


def test_script_with_and_without_separator():
    with_sep = parse_document(
        "---\ngridmd: \"1.0\"\n---\n# S\n```{script} x lang=js\non: manual\n---\ncode();\n```\n"
    )
    fence = with_sep.sheets[0].blocks[0]
    assert fence.meta == {"on": "manual"} and fence.code == "code();"

    no_sep = parse_document("---\ngridmd: \"1.0\"\n---\n# S\n```{script} x lang=js\ncode();\n```\n")
    fence2 = no_sep.sheets[0].blocks[0]
    assert fence2.meta == {} and fence2.code == "code();"


def test_raw_and_x_kind_payload():
    doc = parse_document(
        "---\ngridmd: \"1.0\"\n---\n# S\n```{raw} ooxml part=\"a.xml\"\n<x/>\n```\n"
        "```{x-custom}\nopaque\n```\n"
    )
    raw = doc.sheets[0].blocks[0]
    xk = doc.sheets[0].blocks[1]
    assert raw.payload == "<x/>" and xk.payload == "opaque"


# ---- @ directive body ----
def test_at_multiline_body_dedent_and_trailing_blanks():
    src = "---\ngridmd: \"1.0\"\n---\n# S\n@ A1\n  note: hi\n\n@ A2 5\n"
    doc = parse_document(src)
    a1 = doc.sheets[0].blocks[0]
    assert a1.body == {"note": "hi"}
    # the blank line after the body must not be consumed into A1's body run
    assert doc.sheets[0].blocks[1].target_text == "A2"


def test_at_body_not_a_mapping():
    src = "---\ngridmd: \"1.0\"\n---\n# S\n@ A1\n  - 1\n  - 2\n"
    doc = parse_document(src)
    assert any("must be a YAML mapping" in m for m in _errs(doc))


def test_at_inline_flow_props():
    doc = parse_document("---\ngridmd: \"1.0\"\n---\n# S\n@ A1:B1 { merge: true }\n")
    block = doc.sheets[0].blocks[0]
    assert block.props == {"merge": True} and block.scalar_text is None


def test_at_scalar_with_props_split():
    doc = parse_document('---\ngridmd: "1.0"\n---\n# S\n@ A1 "hi" { color: "#FFFFFF" }\n')
    block = doc.sheets[0].blocks[0]
    assert block.scalar_text == '"hi"' and block.props == {"color": "#FFFFFF"}


def test_at_scalar_only():
    doc = parse_document('---\ngridmd: "1.0"\n---\n# S\n@ A1 =SUM(A2:A3) :: 5\n')
    block = doc.sheets[0].blocks[0]
    assert block.scalar_text == "=SUM(A2:A3) :: 5" and block.props is None


def test_at_cse_formula_not_treated_as_props():
    doc = parse_document('---\ngridmd: "1.0"\n---\n# S\n@ A1 {=SUM(A2:A3)} :: 5\n')
    block = doc.sheets[0].blocks[0]
    assert block.scalar_text == "{=SUM(A2:A3)} :: 5"


def test_at_props_split_that_fails_tryprops_falls_back_to_scalar():
    # a right-edge {...} group that is not a valid ident-key map → treated as scalar
    doc = parse_document('---\ngridmd: "1.0"\n---\n# S\n@ A1 text {not: null}\n')
    block = doc.sheets[0].blocks[0]
    assert block.scalar_text == "text {not: null}"


def test_grid_body_non_pipe_row_reports_error():
    doc = parse_document("---\ngridmd: \"1.0\"\n---\n# S\n```{grid} A1\nnot a pipe row\n```\n")
    assert any("expected a pipe row" in m for m in _errs(doc))
