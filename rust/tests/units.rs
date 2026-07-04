//! Unit tests for the branches the conformance fixtures do not exercise,
//! targeting full line coverage. Grouped by module.

use gridmd::dump::format_number;
use gridmd::model::{build_model, translate_formula, Content};
use gridmd::parser::{
    find_props_split, is_reserved_kind, parse_document, parse_info_args, split_pipe_row, Mode,
};
use gridmd::refs::{col_to_num, num_to_col, parse_cell, parse_target, ref_key, TargetKind, MAX_COL};
use gridmd::scalar::{parse_scalar, split_cached, CachedScalar, Scalar};
use gridmd::validate::is_valid_part_path;
use gridmd::xlsx::write::{build_parts, iso_to_serial, native_cell_count, write_xlsx};
use gridmd::xlsx::zip::{crc32, zip_read, zip_write, ZipEntry};
use gridmd::xlsx::xlsx_to_gridmd;
use gridmd::xml::{decode_entities, parse_xml};
use gridmd::yaml::{load, parse_yaml, try_props, Yaml};
use gridmd::{dump_source, lint};

// ---------- helpers ----------

fn s(body: &str) -> String {
    format!("---\ngridmd: \"0.1\"\n---\n\n# S\n{body}\n")
}
fn errs(src: &str) -> Vec<String> {
    lint(src, Mode::Strict).errors.into_iter().map(|d| d.msg).collect()
}
fn warns(src: &str) -> Vec<String> {
    lint(src, Mode::Strict).warnings.into_iter().map(|d| d.msg).collect()
}
fn has_err(body: &str, sub: &str) {
    let src = s(body);
    assert!(errs(&src).iter().any(|m| m.contains(sub)), "expected error `{sub}` for:\n{body}\ngot: {:?}", errs(&src));
}
fn no_err(body: &str) {
    let src = s(body);
    assert!(errs(&src).is_empty(), "unexpected errors for:\n{body}\ngot: {:?}", errs(&src));
}
fn cell<'a>(m: &'a gridmd::model::Model, sheet: usize, refr: &str) -> Option<&'a Content> {
    let c = parse_cell(refr).unwrap();
    m.sheets[sheet]
        .cells
        .iter()
        .find(|x| x.col == c.col && x.row == c.row)
        .and_then(|x| x.content.as_ref())
}
fn model_of(src: &str) -> gridmd::model::Model {
    let res = lint(src, Mode::Strict);
    build_model(&res.doc)
}

// ---------- refs ----------

#[test]
fn refs_basics() {
    assert_eq!(num_to_col(1), "A");
    assert_eq!(num_to_col(26), "Z");
    assert_eq!(num_to_col(27), "AA");
    assert_eq!(num_to_col(16384), "XFD");
    assert_eq!(col_to_num("A"), 1);
    assert_eq!(col_to_num("XFD"), MAX_COL);
    assert_eq!(ref_key(3, 4), (3, 4));
    // parse_cell edge cases
    assert!(parse_cell("A0").is_none()); // leading-zero / zero row
    assert!(parse_cell("A01").is_none());
    assert!(parse_cell("a1").is_none()); // lowercase
    assert!(parse_cell("XFE1").is_none()); // out of bounds col
    assert!(parse_cell("A1048577").is_none()); // out of bounds row
    assert_eq!(parse_cell("$B$2").unwrap().col, 2);
    assert!(parse_cell("A12345678").is_none()); // >7 digits
    assert!(parse_cell("").is_none());
}

#[test]
fn refs_targets() {
    assert_eq!(parse_target("B2").unwrap().kind, TargetKind::Cell);
    assert_eq!(parse_target("B2:D9").unwrap().kind, TargetKind::Range);
    assert_eq!(parse_target("B:D").unwrap().kind, TargetKind::Cols);
    assert_eq!(parse_target("2:9").unwrap().kind, TargetKind::Rows);
    assert_eq!(parse_target("$A:$C").unwrap().kind, TargetKind::Cols);
    assert_eq!(parse_target("$1:$3").unwrap().kind, TargetKind::Rows);
    let t = parse_target("'Q3 Data'!B2").unwrap();
    assert_eq!(t.sheet.as_deref(), Some("Q3 Data"));
    let t2 = parse_target("Sheet1!B2").unwrap();
    assert_eq!(t2.sheet.as_deref(), Some("Sheet1"));
    assert!(parse_target("A1:B2:C3").is_none()); // 3 parts
    assert!(parse_target("garbage").is_none());
    assert!(parse_target("ZZZ9:1").is_none());
    // col-range clamps order
    let c = parse_target("D:A").unwrap();
    assert_eq!((c.c1, c.c2), (1, 4));
    let r = parse_target("9:2").unwrap();
    assert_eq!((r.r1, r.r2), (2, 9));
}

// ---------- scalar ----------

#[test]
fn scalar_grammar() {
    assert!(matches!(parse_scalar(""), Scalar::Blank));
    assert!(matches!(parse_scalar("1e3"), Scalar::Number(n) if n == 1000.0));
    assert!(matches!(parse_scalar("-12.5"), Scalar::Number(n) if n == -12.5));
    assert!(matches!(parse_scalar("0"), Scalar::Number(n) if n == 0.0));
    assert!(matches!(parse_scalar("00"), Scalar::Text { .. })); // not a JSON number
    assert!(matches!(parse_scalar("true"), Scalar::Boolean(true)));
    assert!(matches!(parse_scalar("FALSE"), Scalar::Boolean(false)));
    assert!(matches!(parse_scalar("2026-07-04"), Scalar::Date(_)));
    assert!(matches!(parse_scalar("2026-07-04T06:00:30"), Scalar::Date(_)));
    assert!(matches!(parse_scalar("12:30:45"), Scalar::Time(_)));
    assert!(matches!(parse_scalar("#N/A"), Scalar::Error(_)));
    assert!(matches!(parse_scalar("#div/0!"), Scalar::Error(e) if e == "#DIV/0!"));
    assert!(matches!(parse_scalar("'0042"), Scalar::Text { value, .. } if value == "0042"));
    assert!(matches!(parse_scalar("\"a\"\"b\""), Scalar::Text { value, .. } if value == "a\"b"));
    assert!(matches!(parse_scalar("\"unterminated"), Scalar::Text { problem: Some(_), .. }));
    assert!(matches!(parse_scalar("\"a\"b\""), Scalar::Text { problem: Some(_), .. }));
    assert!(matches!(parse_scalar("plain words"), Scalar::Text { problem: None, .. }));
}

#[test]
fn scalar_formulas_and_cached() {
    let f = parse_scalar("=SUM(A1:A2) :: 45");
    match f {
        Scalar::Formula { formula, cached: Some(c), cse: false } => {
            assert_eq!(formula, "SUM(A1:A2)");
            assert!(matches!(*c, CachedScalar::Value(Scalar::Number(n)) if n == 45.0));
        }
        _ => panic!("{f:?}"),
    }
    // cached with " :: " inside a string literal must not split there
    let g = parse_scalar("=IF(A1,\"x :: y\",\"z\") :: \"x :: y\"");
    match g {
        Scalar::Formula { formula, cached: Some(c), .. } => {
            assert_eq!(formula, "IF(A1,\"x :: y\",\"z\")");
            assert!(matches!(*c, CachedScalar::Value(Scalar::Text { .. })));
        }
        _ => panic!(),
    }
    // CSE array formula
    assert!(matches!(parse_scalar("{=TRANSPOSE(A1:B2)}"), Scalar::Formula { cse: true, .. }));
    assert!(matches!(parse_scalar("{=UNTERMINATED"), Scalar::Text { problem: Some(_), .. }));
    // cached that is itself a formula → invalid
    let bad = parse_scalar("=A1 :: =B1");
    assert!(matches!(bad, Scalar::Formula { cached: Some(c), .. } if matches!(*c, CachedScalar::Invalid(_))));
    // split_cached with no separator
    assert_eq!(split_cached("plain"), ("plain".to_string(), None));
    assert_eq!(split_cached("a :: b").1, Some("b".to_string()));
}

// ---------- yaml ----------

#[test]
fn yaml_values_and_subset() {
    let l = load("a: 1\nb: 0.5\nc: true\nd: ~\ne: text\nf: [1, 2]\ng: { k: v }\nh: |\n  block\n").unwrap();
    assert!(!l.has_anchor_or_alias && !l.has_tag);
    let v = l.value;
    assert_eq!(v.get("a").and_then(|x| x.as_i64()), Some(1));
    assert!(matches!(v.get("b"), Some(Yaml::Real(_))));
    assert_eq!(v.get("c").and_then(|x| x.as_bool()), Some(true));
    assert!(v.get("d").unwrap().is_null());
    assert_eq!(v.get("e").and_then(|x| x.as_str()), Some("text"));
    assert_eq!(v.get("f").and_then(|x| x.as_array()).map(|a| a.len()), Some(2));
    assert_eq!(v.get("g").and_then(|x| x.get("k")).and_then(|x| x.as_str()), Some("v"));
    assert_eq!(v.get("h").and_then(|x| x.as_str()), Some("block\n"));
    // to_js_string
    assert_eq!(Yaml::Int(3).to_js_string(), "3");
    assert_eq!(Yaml::Real(0.5).to_js_string(), "0.5");
    assert_eq!(Yaml::Bool(false).to_js_string(), "false");
    assert_eq!(Yaml::Null.to_js_string(), "null");
    assert_eq!(Yaml::Str("x".into()).to_js_string(), "x");
    assert_eq!(Yaml::Array(vec![]).to_js_string(), "");
    assert_eq!(Yaml::Hash(vec![]).to_js_string(), "[object Object]");
    // empty document
    assert!(load("").unwrap().value.is_null());
    // scanner error
    assert!(load("a: [1, 2").is_err());
}

#[test]
fn yaml_anchor_tag_props() {
    let a = load("a: &x 1\nb: *x\n").unwrap();
    assert!(a.has_anchor_or_alias);
    let t = load("a: !!str 1\n").unwrap();
    assert!(t.has_tag);
    // parse_yaml surfaces subset violations + parse errors
    let mut e = Vec::new();
    parse_yaml("a: &x 1\nb: *x", 1, &mut e);
    assert!(e.iter().any(|d| d.msg.contains("anchors/aliases")));
    let mut e2 = Vec::new();
    parse_yaml("a: !!str 1", 1, &mut e2);
    assert!(e2.iter().any(|d| d.msg.contains("tags")));
    let mut e3 = Vec::new();
    parse_yaml("[bad", 1, &mut e3);
    assert!(e3.iter().any(|d| d.msg.starts_with("YAML:")));
    let mut e4 = Vec::new();
    assert!(matches!(parse_yaml("   ", 1, &mut e4), Yaml::Hash(_)));
    // try_props
    assert!(try_props("{ a: 1, b: two }").is_some());
    assert!(try_props("{ a: 1, Bad: 2 }").is_none()); // non-ident key
    assert!(try_props("{ a: null }").is_none()); // null value
    assert!(try_props("[1, 2]").is_none()); // not a map
    assert!(try_props("{ x-ext: 1 }").is_some());
    assert!(try_props("not: valid: yaml: [").is_none());
}

// ---------- parser ----------

#[test]
fn parser_props_split_and_pipes() {
    assert_eq!(find_props_split("=A1 { x: 1 }"), ("=A1".into(), Some("{ x: 1 }".into())));
    assert_eq!(find_props_split("no braces"), ("no braces".into(), None));
    assert_eq!(find_props_split("text{a}").1, None); // not preceded by space
    assert_eq!(find_props_split("a } b }").1, None); // unbalanced depth<0
    assert_eq!(find_props_split("{a} tail").1, None); // group not at end
    assert_eq!(split_pipe_row("| a | b |"), Some(vec!["a".into(), "b".into()]));
    assert_eq!(split_pipe_row("| a \\| b |"), Some(vec!["a | b".into()]));
    assert_eq!(split_pipe_row("no pipe"), None);
    assert_eq!(split_pipe_row("| unclosed"), None);
    assert_eq!(split_pipe_row("|"), None);
    assert!(is_reserved_kind("grid"));
    assert!(!is_reserved_kind("nope"));
}

#[test]
fn parser_info_args() {
    let mut e = Vec::new();
    let a = parse_info_args("column \"Q by item\" at H6:M18 size 480x320 lang=js", 1, &mut e);
    assert_eq!(a.positional, vec!["column", "Q by item"]);
    assert_eq!(a.anchor.as_deref(), Some("H6:M18"));
    assert_eq!(a.size, Some((480, 320)));
    assert_eq!(a.flags.get("lang").map(|s| s.as_str()), Some("js"));
    assert!(e.is_empty());
    let mut e2 = Vec::new();
    parse_info_args("size 480", 1, &mut e2);
    assert!(e2.iter().any(|d| d.msg.contains("WxH")));
    let mut e3 = Vec::new();
    parse_info_args("at", 1, &mut e3);
    assert!(e3.iter().any(|d| d.msg.contains("requires an anchor")));
    // quoted flag value (no spaces — whitespace splits tokens first, per JS)
    let mut e4 = Vec::new();
    let a4 = parse_info_args("part=\"ab\"", 1, &mut e4);
    assert_eq!(a4.flags.get("part").map(|s| s.as_str()), Some("ab"));
}

#[test]
fn parser_document_structure() {
    // missing frontmatter
    assert!(errs("no frontmatter").iter().any(|m| m.contains("must begin with")));
    // unterminated frontmatter
    let d = parse_document("---\ngridmd: \"0.1\"\n# S\n", Mode::Strict);
    assert!(d.errors.iter().any(|e| e.msg.contains("unterminated frontmatter")));
    // doc comments + level-2 headings are ignored
    no_err("> a comment\n## a subheading\n@ A1 1");
    // lenient mode: unknown line → warning; strict → error
    let strict = lint(&s("garbage line here"), Mode::Strict);
    assert!(strict.errors.iter().any(|e| e.msg.contains("unrecognized line")));
    let lenient = lint(&s("garbage line here"), Mode::Lenient);
    assert!(lenient.errors.is_empty());
    assert!(lenient.warnings.iter().any(|w| w.msg.contains("unrecognized line")));
    // unclosed fence
    assert!(errs(&s("```{grid} A1\n| a |")).iter().any(|m| m.contains("unclosed")));
    // @ body must be a mapping
    assert!(errs(&s("@ A1\n  - not a map")).iter().any(|m| m.contains("must be a YAML mapping")));
    // props-only inline
    no_err("@ C7 { fill: \"#FDECEC\" }");
    // x- fence round-trips silently
    no_err("```{x-custom}\nanything\n```");
    // script without ---
    no_err("```{script} s lang=js\ncode();\n```");
    // table without ---
    assert!(errs(&s("```{table} T at A1\n| a |\n```")).iter().any(|m| m.contains("`---`-separated")));
}

// ---------- validate: frontmatter ----------

#[test]
fn validate_frontmatter() {
    assert!(errs("---\ngridmd: 0.1\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("gridmd:")));
    assert!(errs("---\nfoo: 1\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("gridmd:")));
    assert!(warns(&format!("---\ngridmd: \"0.1\"\nunknownkey: 1\n---\n# S\n@ A1 1\n")).iter().any(|w| w.contains("unknown frontmatter key")));
    assert!(errs("---\ngridmd: \"0.1\"\ndate-system: 1950\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("date-system")));
    assert!(errs("---\ngridmd: \"0.1\"\ncalc: { mode: turbo }\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("calc.mode")));
    assert!(errs("---\ngridmd: \"0.1\"\nnames:\n  - { name: X, ref: A1, formula: B1 }\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("exactly one of")));
    assert!(errs("---\ngridmd: \"0.1\"\nnames:\n  - { foo: 1 }\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("require a name")));
    assert!(errs("---\ngridmd: \"0.1\"\nnames:\n  - { name: X, ref: A1 }\n  - { name: x, ref: B1 }\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("duplicate defined name")));
    assert!(errs("---\ngridmd: \"0.1\"\nstyles: { hdr: notamap }\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("must be a mapping")));
    assert!(errs("---\ngridmd: \"0.1\"\ntheme: { colors: { accent1: red } }\n---\n# S\n@ A1 1\n").iter().any(|m| m.contains("#RRGGBB")));
    assert!(warns("---\ngridmd: \"0.1\"\ntheme: { colors: { bogus: \"#FFFFFF\" } }\n---\n# S\n@ A1 1\n").iter().any(|w| w.contains("unknown theme color slot")));
}

// ---------- validate: workbook + sheet framing ----------

#[test]
fn validate_workbook_and_sheets() {
    assert!(errs("---\ngridmd: \"0.1\"\n---\n@ A1 1\n").iter().any(|m| m.contains("not allowed before the first sheet")));
    assert!(errs("---\ngridmd: \"0.1\"\n---\n@ A1 1\n").iter().any(|m| m.contains("at least one sheet")));
    assert!(errs("---\ngridmd: \"0.1\"\n---\n```{bogus}\n```\n# S\n@ A1 1\n").iter().any(|m| m.contains("unknown directive")));
    assert!(errs("---\ngridmd: \"0.1\"\n---\n```{grid} A1\n| a |\n```\n# S\n@ A2 1\n").iter().any(|m| m.contains("sheet-scoped")));
    // x- workbook block ignored
    no_err_full("---\ngridmd: \"0.1\"\n---\n```{x-foo}\nx\n```\n# S\n@ A1 1\n");
    // workbook query/script/raw ok
    no_err_full("---\ngridmd: \"0.1\"\n---\n```{query} Q\nsource: { url: x }\n```\n# S\n@ A1 1\n");
    // sheet name rules
    assert!(errs("---\ngridmd: \"0.1\"\n---\n# A/B\n@ A1 1\n").iter().any(|m| m.contains("forbidden character")));
    let long = "x".repeat(40);
    assert!(errs(&format!("---\ngridmd: \"0.1\"\n---\n# {long}\n@ A1 1\n")).iter().any(|m| m.contains("exceeds 31")));
    assert!(errs("---\ngridmd: \"0.1\"\n---\n# S\n@ A1 1\n# s\n@ A1 1\n").iter().any(|m| m.contains("duplicate sheet name")));
}
fn no_err_full(src: &str) {
    assert!(errs(src).is_empty(), "unexpected errors: {:?}", errs(src));
}

// ---------- validate: @ directive ----------

#[test]
fn validate_at_directive() {
    has_err("@ 1 =A", "invalid @ target");
    has_err("@ Other!A1 1", "must name the containing sheet");
    has_err("@ A1 \"unterminated", "scalar:");
    has_err("@ A1 =X :: =Y", "cached value must not be a formula");
    has_err("@ A1 5\n  value: 6", "inline content and body content keys");
    no_err("@ A1 =SUM(B:B)\n  value: 6"); // cached-only body allowed with a formula
    has_err("@ A1:B2 5", "range targets accept formula content only");
    has_err("@ A1 5 { fill: red }", "not a color");
    has_err("@ A1 5 { link: \"ftp://x\" }", "scheme must be https");
    has_err("@ A1 5 { merge: true }", "merge: requires a range target");
    has_err("@ A1:B2 { merge: yes }", "only `true` is valid");
    has_err("@ A1 =X { spill: A1 }", "must be a range");
    has_err("@ A1 =X { spill: B2:C3 }", "must start at the anchor cell");
    has_err("@ A1 5 { rich: notlist }", "must be a list of runs");
    has_err("@ A1 true { control: radio }", "unknown control radio");
    assert!(warns(&s("@ A1 5 { weird: 1 }")).iter().any(|w| w.contains("unknown property")));
    assert!(warns(&s("@ A1\n  formula: =B1")).iter().any(|w| w.contains("formula without a cached value")));
    // relative fill defines the range
    no_err("@ B2:B4 =A2*2");
    // over-cap relative fill only warns
    assert!(warns(&s("@ A1:Z5000 =A1*2")).iter().any(|w| w.contains("overlap checking skipped")));
}

// ---------- validate: fences ----------

#[test]
fn validate_fences() {
    has_err("```{grid}\n| a |\n```", "requires a cell anchor");
    has_err("```{table} 1A at A1\n---\n| a |\n```", "requires a valid table name");
    has_err("```{table} T\n---\n| a |\n```", "requires `at <cell>`");
    has_err("```{table} T at A1\n---\n```", "requires payload rows");
    has_err("```{table} T at A1\ncols: { nope: {} }\n---\n| a | b |\n| 1 | 2 |\n```", "references unknown column");
    has_err("```{table} T at A1\nsort:\n  - { col: zzz }\n---\n| a |\n| 1 |\n```", "sort references unknown column");
    has_err("```{table} T at A1\n---\n| a |  |\n```", "must be non-empty text");
    has_err("```{cf} B2\n- when: \"> 1\"\n  format: { fill: red }\n```", "not a color");
    has_err("```{cf} B2\n- when: 1\n  bars: 2\n```", "exactly one distinguishing key");
    has_err("```{cf} B2\nnotalist: 1\n```", "must be a YAML list");
    has_err("```{cf} B2\n- when: 1\n  priority: 0\n```", "positive integer");
    has_err("```{validation} B2\ntype: bogus\n```", "type must be one of");
    has_err("```{validation} B2\ntype: list\n```", "requires values: or source:");
    has_err("```{validation} B2\ntype: list\nvalues: [a]\nerror: { style: nope }\n```", "error.style");
    has_err("```{filter} B2\n```", "requires a range");
    has_err("```{filter} B2:C3\ncols: { zz: 1 }\n```", "column letters");
    has_err("```{chart} column at A1\n```", "requires series");
    has_err("```{chart} column at A1\nseries:\n  - { name: x }\n```", "requires val:");
    has_err("```{chart} column at A1\nseries:\n  - { val: B1, color: red }\n```", "color");
    assert!(warns(&s("```{chart} bogustype at A1\nseries:\n  - { val: B1 }\n```")).iter().any(|w| w.contains("unknown chart type")));
    has_err("```{sparklines} A1\n```", "requires source:");
    has_err("```{sparklines} A1\nsource: B1\ntype: bogus\n```", "line | column | win-loss");
    has_err("```{pivot} P at A1\n```", "requires source:");
    has_err("```{slicer} at A1\n```", "requires for: and field:");
    has_err("```{image} at A1\n```", "requires src:");
    has_err("```{image} at A1\nsrc: \"javascript:alert(1)\"\n```", "scheme allowlist");
    assert!(warns(&s("```{shape} bogus at A1\n```")).iter().any(|w| w.contains("unknown shape kind")));
    has_err("```{shape} rect\n```", "requires an anchor");
    has_err("```{checkbox} at A1\nlinked: notacell\n```", "linked: must be a cell");
    has_err("```{comments} A1\n- { by: x }\n```", "each comment requires");
    has_err("```{comments} A1\nnotalist: 1\n```", "must be a YAML list");
    has_err("```{outline}\nrows:\n  - { range: bad }\n```", "must be \"n:m\"");
    has_err("```{outline}\ncols:\n  - { range: bad }\n```", "must be \"A:B\"");
    has_err("```{page}\norientation: sideways\n```", "portrait | landscape");
    has_err("```{page}\nscale: 100\nfit: { width: 1 }\n```", "mutually exclusive");
    has_err("```{scenario} Sc\n```", "requires cells:");
    has_err("```{scenario} Sc\ncells: { notacell: 1 }\n```", "must be a cell");
    // textbox/checkbox/slicer/image happy anchors
    no_err("```{textbox} at A1\ntext: hi\n```");
}

#[test]
fn validate_raw_and_script() {
    has_err("```{raw} bogus\ndata\n```", "format must be ooxml");
    has_err("```{raw} ooxml part=\"../evil\"\ndata\n```", "package-path");
    has_err("```{raw} ooxml part=\"x/y.xml\" encoding=hex\ndata\n```", "encoding must be base64");
    no_err("```{raw} ooxml part=\"customXml/item1.xml\"\n<x/>\n```");
    has_err("```{script} s\n---\ncode\n```", "requires lang=");
    has_err("```{script} s lang=js\n---\n\n```", "code payload");
    has_err("```{query} Q\nsource: x\nsteps: notalist\n```", "steps: must be a list");
}

#[test]
fn validate_sheet_meta_and_spillcache() {
    has_err("```{sheet}\nkind: bogus\n```", "kind must be worksheet");
    has_err("```{sheet}\ntab-color: red\n```", "tab-color");
    has_err("```{sheet}\nhidden: maybe\n```", "hidden must be false");
    has_err("```{sheet}\nfreeze: notacell\n```", "must be a cell reference");
    has_err("```{sheet}\ncols: { 1: 10 }\n```", "cols key must be a column");
    has_err("```{sheet}\ncols: { A: notmap }\n```", "must be a width or a mapping");
    has_err("```{sheet}\nrows: { A: {} }\n```", "rows key must be a row");
    assert!(warns(&s("```{sheet}\nfoo: 1\n```")).iter().any(|w| w.contains("unknown {sheet} key")));
    // multiple sheet blocks + not-first warning
    has_err("```{sheet}\n```\n```{sheet}\n```", "multiple {sheet} blocks");
    assert!(warns(&s("@ A1 1\n```{sheet}\n```")).iter().any(|w| w.contains("should be the first block")));
    // chart sheet rules
    has_err("```{sheet}\nkind: chart\n```", "requires exactly one {chart}");
    has_err("```{sheet}\nkind: chart\n```\n```{chart} column at sheet\nseries:\n  - { val: B1 }\n```\n@ A1 1", "cannot carry worksheet grid content");
    has_err("```{chart} column at sheet\nseries:\n  - { val: B1 }\n```", "require {sheet} kind: chart");
    // spill-cache orphan + exceeds
    has_err("```{spill-cache}\n| a |\n```", "requires a cell anchor");
    has_err("@ A1 =X { spill: A1:A2 }\n```{spill-cache} A1\n| a |\n| b |\n| c |\n```", "exceeds the declared spill");
    no_err("@ A1 =X { spill: A1:A3 }\n```{spill-cache} A1\n| a |\n| b |\n```");
    // grid cell problem
    has_err("```{grid} A1\n| \"bad |\n```", "grid cell:");
    // valid part path helper
    assert!(is_valid_part_path("customXml/item1.xml"));
    assert!(!is_valid_part_path("/abs"));
    assert!(!is_valid_part_path("a/../b"));
    assert!(!is_valid_part_path("a%2e%2e/b"));
    assert!(!is_valid_part_path(""));
}

// ---------- model ----------

#[test]
fn model_translate_and_bodies() {
    assert_eq!(translate_formula("A1+$B$2*C3", 1, 1), "B2+$B$2*D4");
    assert_eq!(translate_formula("\"lit A1\"&'sheet A1'!Z9", 1, 0), "\"lit A1\"&'sheet A1'!Z10");
    assert_eq!(translate_formula("SUM(A1)", 0, 0), "SUM(A1)"); // A1 followed by ) is not a ref
    assert_eq!(translate_formula("A1", -5, -5), "A1"); // clamps to 1

    let m = model_of(&s("@ B2:B4 =A2*2"));
    assert!(matches!(cell(&m, 0, "B2"), Some(c) if c.formula.as_deref() == Some("A2*2")));
    assert!(matches!(cell(&m, 0, "B4"), Some(c) if c.formula.as_deref() == Some("A4*2")));

    let m2 = model_of(&s("@ B2\n  formula: =SUM(A1:A2)\n  value: 5"));
    let c = cell(&m2, 0, "B2").unwrap();
    assert_eq!(c.formula.as_deref(), Some("SUM(A1:A2)"));
    assert!(matches!(c.cached, Some(Scalar::Number(n)) if n == 5.0));

    // body value date/time/bool/number coercion + rich + entity
    assert!(matches!(cell(&model_of(&s("@ A1\n  value: 2026-01-01")), 0, "A1").unwrap().scalar, Some(Scalar::Date(_))));
    assert!(matches!(cell(&model_of(&s("@ A1\n  value: 12:30")), 0, "A1").unwrap().scalar, Some(Scalar::Time(_))));
    assert!(matches!(cell(&model_of(&s("@ A1\n  value: true")), 0, "A1").unwrap().scalar, Some(Scalar::Boolean(true))));
    assert!(matches!(cell(&model_of(&s("@ A1\n  value: 7")), 0, "A1").unwrap().scalar, Some(Scalar::Number(n)) if n == 7.0));
    let rich = model_of(&s("@ A1\n  rich:\n    - { text: \"a\" }\n    - { text: \"b\" }"));
    assert_eq!(cell(&rich, 0, "A1").unwrap().rich.as_deref(), Some("ab"));
    let ent = model_of(&s("@ A1\n  entity: { type: stock, id: MSFT, text: Micro }"));
    assert!(matches!(cell(&ent, 0, "A1").unwrap().scalar, Some(Scalar::Text { ref value, .. }) if value == "Micro"));

    // style extend resolution (expand_patch → resolve_style recursion)
    let styled = format!(
        "---\ngridmd: \"0.1\"\nstyles:\n  base: {{ bold: true }}\n  hdr: {{ extend: base, italic: true }}\n---\n\n# S\n@ A1:B1 {{ merge: true, style: hdr }}\n@ A1 \"x\"\n"
    );
    let sm = model_of(&styled);
    assert_eq!(sm.sheets[0].merges.len(), 1);
}

// ---------- dump ----------

#[test]
fn dump_number_formatting() {
    assert_eq!(format_number(3.0), "3");
    assert_eq!(format_number(1000.0), "1000");
    assert_eq!(format_number(-0.0), "0");
    assert_eq!(format_number(0.3), "0.3");
    assert_eq!(format_number(442.1), "442.1");
    assert_eq!(format_number(-12.5), "-12.5");
    assert_eq!(format_number(0.3771428571428571), "0.3771428571428571");
    // non-finite falls back to Display
    assert_eq!(format_number(f64::INFINITY), "inf");
    // dump_source Err path
    assert!(dump_source("bad").is_err());
}

// ---------- xlsx: zip + serials + carry ----------

#[test]
fn xlsx_zip_round_trip() {
    let entries = vec![
        ZipEntry { name: "a.txt".into(), data: b"hello".to_vec() },
        ZipEntry { name: "b/c.txt".into(), data: vec![] },
    ];
    let buf = zip_write(&entries);
    let read = zip_read(&buf).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].0, "a.txt");
    assert_eq!(read[0].1, b"hello");
    assert_eq!(crc32(b""), 0);
    assert!(zip_read(b"not a zip at all").is_err());
    assert!(zip_read(&[]).is_err());
}

#[test]
fn xlsx_iso_to_serial() {
    assert_eq!(iso_to_serial("1900-01-01", 1900), 1.0);
    assert_eq!(iso_to_serial("1900-02-28", 1900), 59.0);
    assert_eq!(iso_to_serial("1900-03-01", 1900), 61.0);
    assert_eq!(iso_to_serial("1904-01-01", 1904), 0.0);
    assert_eq!(iso_to_serial("1905-01-01", 1904), 366.0); // 1904 is a leap year
    assert!((iso_to_serial("12:30", 1900) - 0.520833333333).abs() < 1e-9);
    assert!((iso_to_serial("2026-01-01T06:00", 1900) - iso_to_serial("2026-01-01", 1900) - 0.25).abs() < 1e-9);
}

#[test]
fn xlsx_native_import_without_carry() {
    // Build parts WITHOUT the carry part → forces the native reverse-parser.
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/fixtures/01-cells.gmd"),
    )
    .unwrap();
    let m = model_of(&src);
    assert!(native_cell_count(&m) > 0);
    let parts = build_parts(&m, &src, false);
    assert!(!parts.iter().any(|p| p.name == "gridmd/source.gmd"));
    let xlsx = zip_write(&parts);
    let (gmd, report) = xlsx_to_gridmd(&xlsx).unwrap();
    assert!(report.iter().any(|r| r.action == "imported"));
    assert!(lint(&gmd, Mode::Strict).errors.is_empty(), "native import lint: {:?}", lint(&gmd, Mode::Strict).errors);
    // carry path still works
    let with_carry = write_xlsx(&m, &src);
    let (restored, rep2) = xlsx_to_gridmd(&with_carry).unwrap();
    assert_eq!(restored, src);
    assert!(rep2.iter().any(|r| r.action == "restored"));
}

// ---------- xml ----------

#[test]
fn xml_parser() {
    let doc = parse_xml("<?xml version=\"1.0\"?><!DOCTYPE x><root a=\"1\" r:id=\"z\"><!-- c --><child/><child>t&amp;<![CDATA[<raw>]]></child></root>");
    assert_eq!(doc.name, "root");
    assert_eq!(doc.attr("a"), Some("1"));
    assert_eq!(doc.attr("id"), Some("z")); // local-name lookup for r:id
    assert_eq!(doc.all("child").len(), 2);
    assert_eq!(doc.one("child").map(|c| c.name.as_str()), Some("child"));
    assert_eq!(doc.all("child")[1].text_of(), "t&<raw>");
    assert_eq!(decode_entities("&lt;&gt;&amp;&quot;&apos;&#65;&#x42;&unknown;"), "<>&\"'AB&unknown;");
    // empty / unclosed inputs don't panic
    let _ = parse_xml("");
    let _ = parse_xml("<a><b>");
}
