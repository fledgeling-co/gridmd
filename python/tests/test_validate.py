"""Strict validation (validate) — every error and warning branch."""

from __future__ import annotations

from gridmd import lint
from gridmd.validate import (
    is_safe_image_src,
    is_valid_part_path,
)


def errs(body: str, front: str = 'gridmd: "1.0"') -> list[str]:
    return [e.msg for e in lint(f"---\n{front}\n---\n# S\n{body}").errors]


def warns(body: str, front: str = 'gridmd: "1.0"') -> list[str]:
    return [w.msg for w in lint(f"---\n{front}\n---\n# S\n{body}").warnings]


def full_errs(src: str) -> list[str]:
    return [e.msg for e in lint(src).errors]


def has(msgs: list[str], needle: str) -> bool:
    return any(needle in m for m in msgs)


# ---- helper predicates ----
def test_is_valid_part_path():
    assert is_valid_part_path("customXml/item1.xml")
    assert not is_valid_part_path("")
    assert not is_valid_part_path(5)
    assert not is_valid_part_path("/abs")
    assert not is_valid_part_path("a\\b")
    assert not is_valid_part_path("a b")
    assert not is_valid_part_path("a/%2e/b")
    assert not is_valid_part_path("a/../b")
    assert not is_valid_part_path("a/./b")


def test_is_safe_image_src():
    assert is_safe_image_src("assets/x.png")
    assert is_safe_image_src("https://e.com/x.png")
    assert is_safe_image_src("data:image/png;base64,AAA")
    assert not is_safe_image_src(5)
    assert not is_safe_image_src("javascript:alert(1)")
    assert not is_safe_image_src("data:text/html,x")
    assert not is_safe_image_src("http://e.com/x.png")


# ---- frontmatter ----
def test_frontmatter_gridmd_required():
    assert has(full_errs("---\ntitle: x\n---\n# S\n@ A1 1\n"), "gridmd:")


def test_frontmatter_unknown_key_warns():
    assert has(warns("@ A1 1\n", front='gridmd: "1.0"\nbogus: 1'), "unknown frontmatter key")


def test_frontmatter_date_system_bad():
    assert has(errs("@ A1 1\n", front='gridmd: "1.0"\ndate-system: 1899'), "date-system must be")


def test_frontmatter_calc_mode_bad():
    assert has(errs("@ A1 1\n", front='gridmd: "1.0"\ncalc: { mode: weird }'), "calc.mode must be")


def test_frontmatter_names_variants():
    assert has(errs("@ A1 1\n", front='gridmd: "1.0"\nnames:\n  - 5'), "names entries require a name")
    assert has(errs("@ A1 1\n", front='gridmd: "1.0"\nnames:\n  - { name: A }'), "exactly one of")
    dup = 'gridmd: "1.0"\nnames:\n  - { name: A, ref: S!A1 }\n  - { name: A, formula: X }'
    assert has(errs("@ A1 1\n", front=dup), "duplicate defined name")


def test_frontmatter_style_not_mapping():
    assert has(errs("@ A1 1\n", front='gridmd: "1.0"\nstyles:\n  h: 5'), "must be a mapping")


def test_frontmatter_theme_colors():
    assert has(warns("@ A1 1\n", front='gridmd: "1.0"\ntheme: { colors: { weird: "#FFFFFF" } }'), "unknown theme color slot")
    assert has(errs("@ A1 1\n", front='gridmd: "1.0"\ntheme: { colors: { accent1: red } }'), "must be #RRGGBB")


# ---- sheets ----
def test_zero_sheets():
    assert has(full_errs('---\ngridmd: "1.0"\n---\n'), "requires at least one sheet")


def test_sheet_name_too_long():
    assert has(full_errs('---\ngridmd: "1.0"\n---\n# ' + "x" * 32 + "\n@ A1 1\n"), "exceeds 31 chars")


def test_sheet_name_forbidden_char():
    assert has(full_errs('---\ngridmd: "1.0"\n---\n# Bad:Name\n@ A1 1\n'), "forbidden character")


def test_duplicate_sheet_name():
    assert has(full_errs('---\ngridmd: "1.0"\n---\n# S\n@ A1 1\n# s\n@ A1 1\n'), "duplicate sheet name")


# ---- grid ----
def test_grid_needs_anchor():
    assert has(errs("```{grid}\n| 1 |\n```\n"), "requires a cell anchor")


def test_grid_cell_problem():
    assert has(errs('```{grid} A1\n| "oops |\n```\n'), "grid cell:")


# ---- table ----
def test_table_bad_name():
    assert has(errs("```{table} A1 at A1\n---\n| a |\n| 1 |\n```\n"), "requires a valid table name")


def test_table_missing_anchor():
    assert has(errs("```{table} T\n---\n| a |\n| 1 |\n```\n"), "requires `at <cell>`")


def test_table_no_rows():
    assert has(errs("```{table} T at A1\n---\n```\n"), "requires payload rows")


def test_table_duplicate_column():
    assert has(errs("```{table} T at A1\n---\n| a | a |\n| 1 | 2 |\n```\n"), "duplicate table column name")


def test_table_header_non_text():
    assert has(errs("```{table} T at A1\n---\n| 1 | b |\n| x | y |\n```\n"), "header cells must be non-empty text")


def test_table_unknown_column_refs():
    body = (
        "```{table} T at A1\n"
        "cols: { nope: { numfmt: \"0\" } }\n"
        "total: { gone: X }\n"
        "filter: { missing: { values: [x] } }\n"
        "sort:\n  - { col: absent }\n"
        "---\n| a | b |\n| 1 | 2 |\n```\n"
    )
    e = errs(body)
    assert has(e, "cols references unknown column")
    assert has(e, "total references unknown column")
    assert has(e, "filter references unknown column")
    assert has(e, "sort references unknown column")


def test_table_name_collision():
    body = "```{table} Dup at A1\n---\n| a |\n| 1 |\n```\n```{table} Dup at C1\n---\n| b |\n| 2 |\n```\n"
    assert has(errs(body), "table name collides")


def test_table_with_total_defines_total_row():
    body = "```{table} T at A1\ntotal:\n  qty: =SUBTOTAL(109,[qty])\n---\n| item | qty |\n| a | 1 |\n```\n"
    assert errs(body) == []


# ---- cf ----
def test_cf_branches():
    assert has(errs("```{cf} nope\n- when: \"> 5\"\n```\n"), "invalid target")
    assert has(errs("```{cf} A1\nfoo: bar\n```\n"), "body must be a YAML list")
    assert has(errs("```{cf} A1\n- when: 1\n  bars: {}\n```\n"), "exactly one distinguishing key")
    assert has(errs("```{cf} A1\n- when: 1\n  priority: 0\n```\n"), "priority must be a positive integer")
    assert has(errs('```{cf} A1\n- when: 1\n  format: { fill: nope }\n```\n'), "not a color")


# ---- validation ----
def test_validation_branches():
    assert has(errs("```{validation} nope\ntype: list\nvalues: [a]\n```\n"), "invalid target")
    assert has(errs("```{validation} A1\ntype: weird\n```\n"), "type must be one of")
    assert has(errs("```{validation} A1\ntype: list\n```\n"), "requires values: or source:")
    assert has(errs("```{validation} A1\ntype: list\nvalues: [a]\nerror: { style: bad }\n```\n"), "error.style must be")


# ---- filter ----
def test_filter_branches():
    assert has(errs("```{filter} A1\n```\n"), "requires a range")
    assert has(errs("```{filter} A1:B2\ncols: { zz9: { values: [x] } }\n```\n"), "filter cols keys are column letters")


# ---- chart ----
def test_chart_branches():
    assert has(warns("```{chart} weird at A1\nseries:\n  - { val: A1 }\n```\n"), "unknown chart type")
    assert has(errs("```{chart} column\nseries:\n  - { val: A1 }\n```\n"), "requires `at <anchor>`")
    assert has(errs("```{chart} column at A1\nseries:\n  - { name: x }\n```\n"), "requires val:")
    assert has(errs('```{chart} column at A1\nseries:\n  - { val: A1, color: nope }\n```\n'), "color: not a color")
    assert has(errs("```{chart} column at nope\nseries:\n  - { val: A1 }\n```\n"), "invalid target")
    assert has(errs("```{chart} column at A1\n```\n"), "requires series:, data:, or pivot:")


# ---- sparklines / pivot / slicer / image / shape / textbox / checkbox ----
def test_sparklines():
    assert has(errs("```{sparklines} nope\nsource: A1\n```\n"), "invalid target")
    assert has(errs("```{sparklines} A1\n```\n"), "requires source:")
    assert has(errs("```{sparklines} A1\nsource: A1\ntype: weird\n```\n"), "type must be line")


def test_pivot():
    assert has(errs("```{pivot} P at zz\nsource: T\n```\n"), "requires `at <cell>`")
    assert has(errs("```{pivot} P at A1\n```\n"), "requires source:")
    coll = "```{table} P at A1\n---\n| a |\n| 1 |\n```\n```{pivot} P at C1\nsource: P\n```\n"
    assert has(errs(coll), "pivot name collides")


def test_slicer():
    assert has(errs("```{slicer}\nfor: T\nfield: x\n```\n"), "requires an anchor")
    assert has(errs("```{slicer} at A1\n```\n"), "requires for: and field:")


def test_image():
    assert has(errs("```{image}\nsrc: a.png\n```\n"), "requires an anchor")
    assert has(errs("```{image} at A1\n```\n"), "requires src:")
    assert has(errs("```{image} at A1\nsrc: javascript:x\n```\n"), "scheme allowlist")


def test_shape_and_textbox():
    assert has(warns("```{shape} squiggle at A1\n```\n"), "unknown shape kind")
    assert has(errs("```{shape} rect\n```\n"), "requires an anchor")
    assert has(errs("```{textbox}\ntext: x\n```\n"), "requires an anchor")


def test_checkbox():
    assert has(errs("```{checkbox}\n```\n"), "requires an anchor")
    assert has(errs("```{checkbox} at A1\nlinked: notacell\n```\n"), "linked: must be a cell")


# ---- comments / outline / page ----
def test_comments():
    assert has(errs("```{comments} A1:B2\n- { by: x, at: y, text: z }\n```\n"), "requires a cell target")
    assert has(errs("```{comments} A1\nfoo: bar\n```\n"), "body must be a YAML list")
    assert has(errs("```{comments} A1\n- { by: x }\n```\n"), "each comment requires")


def test_outline():
    assert has(errs("```{outline}\nrows:\n  - { range: nope }\n```\n"), 'rows range must be "n:m"')
    assert has(errs("```{outline}\ncols:\n  - { range: nope }\n```\n"), 'cols range must be "A:B"')


def test_page():
    assert has(errs("```{page}\norientation: sideways\n```\n"), "orientation must be")
    assert has(errs("```{page}\nscale: 80\nfit: { width: 1 }\n```\n"), "mutually exclusive")


# ---- scenario / raw ----
def test_scenario():
    assert has(errs("```{scenario}\ncells: { B2: 1 }\n```\n"), "requires a name")
    assert has(errs("```{scenario} S\n```\n"), "requires cells:")
    assert has(errs("```{scenario} S\ncells: { notacell: 1 }\n```\n"), "scenario cells key must be a cell")


def test_raw_sheet_scope():
    assert has(errs("```{raw} weird\nx\n```\n"), "format must be ooxml")
    assert has(errs('```{raw} ooxml part="/bad"\nx\n```\n'), "package-path canonicalization")
    assert has(errs('```{raw} ooxml part="a.xml" encoding="gzip"\nx\n```\n'), "encoding must be base64")


# ---- workbook-level ----
def test_at_before_sheet():
    assert has(full_errs('---\ngridmd: "1.0"\n---\n@ A1 1\n# S\n@ A1 1\n'), "not allowed before the first sheet")


def test_unknown_directive_workbook():
    assert has(full_errs('---\ngridmd: "1.0"\n---\n```{bogus}\nx\n```\n# S\n@ A1 1\n'), "unknown directive")


def test_sheet_scoped_before_sheet():
    src = '---\ngridmd: "1.0"\n---\n```{grid} A1\n| 1 |\n```\n# S\n@ A1 1\n'
    assert has(full_errs(src), "sheet-scoped and cannot appear before the first sheet")


def test_x_kind_workbook_skipped():
    src = '---\ngridmd: "1.0"\n---\n```{x-thing}\nopaque\n```\n# S\n@ A1 1\n'
    assert full_errs(src) == []


def test_workbook_query_script_raw_valid():
    src = (
        '---\ngridmd: "1.0"\n---\n'
        "```{query} Q\nsource: { url: x }\nsteps: []\n```\n"
        "```{script} Sc lang=js\non: manual\n---\ncode();\n```\n"
        '```{raw} ooxml part="a.xml"\n<x/>\n```\n'
        "# S\n@ A1 1\n"
    )
    assert full_errs(src) == []


def test_workbook_query_script_errors():
    assert has(full_errs('---\ngridmd: "1.0"\n---\n```{query}\nsteps: 5\n```\n# S\n@ A1 1\n'), "requires a name")
    assert has(full_errs('---\ngridmd: "1.0"\n---\n```{query} Q\nsteps: 5\n```\n# S\n@ A1 1\n'), "steps: must be a list")
    assert has(full_errs('---\ngridmd: "1.0"\n---\n```{script} S\n---\ncode\n```\n# S\n@ A1 1\n'), "requires lang=")
    assert has(full_errs('---\ngridmd: "1.0"\n---\n```{script} S lang=js\n```\n# S\n@ A1 1\n'), "requires a code payload")


# ---- @ directive semantics ----
def test_at_invalid_target():
    assert has(errs("@ nope 1\n"), "invalid @ target")


def test_at_sheet_qualifier_mismatch():
    assert has(errs("@ Other!A1 1\n"), "must name the containing sheet")


def test_at_inline_and_body_conflict():
    assert has(errs("@ A1 5\n  value: 6\n"), "inline content and body content keys")


def test_at_cached_only_exception_ok():
    assert errs("@ A1 =SUM(B1:B3)\n  value: 6\n") == []


def test_at_scalar_problem_and_cached_invalid():
    assert has(errs('@ A1 "oops\n'), "unterminated quoted text")
    assert has(errs("@ A1 =A1 :: =B1\n"), "cached value must not be a formula")


def test_at_range_non_formula_content():
    assert has(errs("@ A1:B2 5\n"), "range targets accept formula content only")


def test_at_fill_under_cap_defines_cells():
    assert errs("@ A1:B2 =C1\n") == []


def test_at_fill_over_cap_warns():
    assert has(warns("@ A1:Z1000 =C1\n"), "overlap checking skipped")


def test_at_unknown_property_warns():
    assert has(warns("@ A1 5 { wat: 1 }\n"), "unknown property")


def test_at_color_and_link_and_merge():
    assert has(errs("@ A1 5 { fill: nope }\n"), "not a color")
    assert has(errs('@ A1 5 { link: "ftp://e.com" }\n'), "scheme must be https")
    assert has(errs("@ A1 5 { merge: true }\n"), "merge: requires a range target")
    assert has(errs("@ A1:B1 5 { merge: yes }\n"), "merge: only `true` is valid")


def test_at_spill_array():
    assert has(errs("@ A1 =X { spill: notarange }\n"), "must be a range")
    assert has(errs("@ A1 =X { spill: B2:C3 }\n"), "range must start at the anchor cell")
    assert errs("@ A1 =X { spill: A1:A3 }\n") == []


def test_at_rich_and_control():
    assert has(errs("@ A1 { rich: 5 }\n"), "rich: must be a list of runs")
    assert has(errs("@ A1 5 { control: dial }\n"), "unknown control")


def test_at_body_formula_without_value_warns():
    assert has(warns("@ A1\n  formula: =SUM(B1:B2)\n"), "readers will need a calc engine")


# ---- sheet meta ----
def test_sheet_meta_branches():
    assert has(warns("```{sheet}\nbogus: 1\n```\n"), "unknown {sheet} key")
    assert has(errs("```{sheet}\nkind: neither\n```\n"), "kind must be worksheet")
    assert has(errs('```{sheet}\ntab-color: red\n```\n'), "tab-color: not a color")
    assert has(errs("```{sheet}\nhidden: maybe\n```\n"), "hidden must be false")
    assert has(errs("```{sheet}\nfreeze: notacell\n```\n"), "must be a cell reference")
    assert has(errs("```{sheet}\ncols: { 9: 10 }\n```\n"), "cols key must be a column")
    assert has(errs("```{sheet}\ncols: { A: nope }\n```\n"), "must be a width or a mapping")
    assert has(errs("```{sheet}\nrows: { x: 10 }\n```\n"), "rows key must be a row")


def test_multiple_sheet_blocks():
    assert has(errs("```{sheet}\nfreeze: A2\n```\n```{sheet}\nfreeze: A3\n```\n"), "multiple {sheet} blocks")


def test_sheet_block_not_first_warns():
    assert has(warns("@ A1 1\n```{sheet}\nfreeze: A2\n```\n"), "{sheet} should be the first block")


# ---- chart sheets ----
def test_chart_sheet_requires_one_chart():
    src = '---\ngridmd: "1.0"\n---\n# S\n```{sheet}\nkind: chart\n```\n'
    assert has(full_errs(src), "requires exactly one")


def test_chart_sheet_no_grid_content():
    src = (
        '---\ngridmd: "1.0"\n---\n# S\n```{sheet}\nkind: chart\n```\n'
        '```{chart} column at sheet\nseries:\n  - { val: A1 }\n```\n@ A1 5\n'
    )
    assert has(full_errs(src), "cannot carry worksheet grid content")


def test_at_sheet_chart_requires_chart_kind():
    src = '---\ngridmd: "1.0"\n---\n# S\n```{chart} column at sheet\nseries:\n  - { val: A1 }\n```\n'
    assert has(full_errs(src), "require {sheet} kind: chart")


# ---- spill-cache ----
def test_spill_cache_no_owner():
    assert has(errs("```{spill-cache} D2\n| 1 |\n```\n"), "has no owning spill")


def test_spill_cache_exceeds_range():
    body = "@ A1 =X { spill: A1:A2 }\n```{spill-cache} A1\n| 1 |\n| 2 |\n| 3 |\n```\n"
    assert has(errs(body), "exceeds the declared spill/array range")


def test_spill_cache_no_anchor():
    assert has(errs("```{spill-cache}\n| 1 |\n```\n"), "requires a cell anchor")


# ---- define-once ----
def test_duplicate_cell_definition():
    assert has(errs("@ A1 1\n@ A1 2\n"), "cell defined more than once")


def test_cell_out_of_bounds():
    assert has(errs("```{grid} XFD1048576\n| 1 | 2 |\n```\n"), "out of bounds")


# ---- remaining branch coverage ----
def test_chart_suffixed_type_is_valid():
    # chart_base_type strips -3d/-stacked before the type check → no warning
    assert warns("```{chart} column-3d at A1\nseries:\n  - { val: A1 }\n```\n") == []


def test_table_body_cell_problem():
    assert has(errs('```{table} T at A1\n---\n| a |\n| "oops |\n```\n'), "table cell:")


def test_fence_target_sheet_qualifier_mismatch():
    assert has(errs("```{cf} Other!A1:B2\n- when: 1\n```\n"), "must name the containing sheet")


def test_sheet_scope_x_kind_skipped():
    assert errs("```{x-foo}\nopaque\n```\n@ A1 1\n") == []


def test_sheet_scope_unknown_directive():
    assert has(errs("```{bogus}\nx\n```\n@ A1 1\n"), "unknown directive")
