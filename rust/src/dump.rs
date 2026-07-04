//! Canonical model dump — the cross-language conformance contract
//! (conformance/README.md). Byte-identical to `JSON.stringify(v, null, 1)` of
//! `js/src/dump.js`'s output. Port of `js/src/dump.js` plus a hand-rolled JSON
//! serializer with ECMAScript `Number → String` formatting.

use crate::model::{CellModel, Content, Model, SheetModel};
use crate::refs::num_to_col;
use crate::scalar::Scalar;
use crate::yaml::Yaml;

/// A minimal JSON value with ordered object keys (never alphabetized).
enum J {
    Null,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

/// ECMAScript `Number → String`: integer-valued doubles print without a decimal
/// point; others use the shortest round-trip decimal. (Rust's `{}` is the same
/// shortest algorithm; it never uses exponential notation, matching ECMAScript
/// for every value in the conformance range — see the README divergence note.)
pub fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
        return (n as i64).to_string();
    }
    format!("{n}")
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{09}' => out.push_str("\\t"),
            '\u{0a}' => out.push_str("\\n"),
            '\u{0c}' => out.push_str("\\f"),
            '\u{0d}' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_indent(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push(' ');
    }
}

fn write_json(j: &J, depth: usize, out: &mut String) {
    match j {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Int(n) => out.push_str(&n.to_string()),
        J::Num(f) => out.push_str(&format_number(*f)),
        J::Str(s) => write_json_string(s, out),
        J::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                push_indent(out, depth + 1);
                write_json(item, depth + 1, out);
            }
            out.push('\n');
            push_indent(out, depth);
            out.push(']');
        }
        J::Obj(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                push_indent(out, depth + 1);
                write_json_string(k, out);
                out.push_str(": ");
                write_json(v, depth + 1, out);
            }
            out.push('\n');
            push_indent(out, depth);
            out.push('}');
        }
    }
}

/// `x ?? null` for a frontmatter string field. Non-string, non-null values are
/// coerced to their `String(x)` form (never happens for spec-valid input, where
/// these fields are always strings).
fn fm_str_field(v: Option<&Yaml>) -> J {
    match v {
        None | Some(Yaml::Null) => J::Null,
        Some(x) => J::Str(x.to_js_string()),
    }
}

fn scalar_dump(s: &Option<Scalar>) -> J {
    match s {
        None => J::Null,
        Some(Scalar::Number(n)) => J::Obj(vec![
            ("t".to_string(), J::Str("n".to_string())),
            ("v".to_string(), J::Num(*n)),
        ]),
        Some(Scalar::Boolean(b)) => J::Obj(vec![
            ("t".to_string(), J::Str("b".to_string())),
            ("v".to_string(), J::Bool(*b)),
        ]),
        Some(Scalar::Error(e)) => J::Obj(vec![
            ("t".to_string(), J::Str("e".to_string())),
            ("v".to_string(), J::Str(e.clone())),
        ]),
        Some(Scalar::Date(d)) | Some(Scalar::Time(d)) => J::Obj(vec![
            ("t".to_string(), J::Str("d".to_string())),
            ("v".to_string(), J::Str(d.clone())),
        ]),
        Some(Scalar::Text { value, .. }) => J::Obj(vec![
            ("t".to_string(), J::Str("s".to_string())),
            ("v".to_string(), J::Str(value.clone())),
        ]),
        // Blank / (defensively) a formula: default `{t:'s', v:String(value ?? '')}`.
        Some(Scalar::Blank) => J::Obj(vec![
            ("t".to_string(), J::Str("s".to_string())),
            ("v".to_string(), J::Str(String::new())),
        ]),
        Some(Scalar::Formula { .. }) => J::Obj(vec![
            ("t".to_string(), J::Str("s".to_string())),
            ("v".to_string(), J::Str(String::new())),
        ]),
    }
}

fn cell_dump(c: &Content) -> J {
    if let Some(rich) = &c.rich {
        return J::Obj(vec![
            ("t".to_string(), J::Str("rich".to_string())),
            ("v".to_string(), J::Str(rich.clone())),
        ]);
    }
    if let Some(formula) = &c.formula {
        return J::Obj(vec![
            ("t".to_string(), J::Str("f".to_string())),
            ("f".to_string(), J::Str(formula.clone())),
            ("cached".to_string(), scalar_dump(&c.cached)),
            (
                "array".to_string(),
                match &c.array_ref {
                    Some(a) => J::Str(a.clone()),
                    None => J::Null,
                },
            ),
        ]);
    }
    scalar_dump(&c.scalar)
}

fn names_dump(fm: &Yaml) -> J {
    let list = fm.get("names").and_then(|n| n.as_array()).unwrap_or(&[]);
    let mut entries: Vec<(String, J)> = Vec::new();
    for n in list {
        let name = n.get("name").map(|v| v.to_js_string()).unwrap_or_default();
        let obj = J::Obj(vec![
            ("name".to_string(), J::Str(name.clone())),
            ("ref".to_string(), fm_str_field(n.get("ref"))),
            ("formula".to_string(), fm_str_field(n.get("formula"))),
            (
                "value".to_string(),
                match n.get("value") {
                    Some(v) => J::Str(v.to_js_string()),
                    None => J::Null,
                },
            ),
        ]);
        entries.push((name, obj));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    J::Arr(entries.into_iter().map(|(_, j)| j).collect())
}

fn cells_dump(s: &SheetModel) -> J {
    let mut cells: Vec<&CellModel> = s.cells.iter().filter(|c| c.content.is_some()).collect();
    cells.sort_by(|a, b| a.row.cmp(&b.row).then(a.col.cmp(&b.col)));
    let mut pairs: Vec<(String, J)> = Vec::new();
    for c in cells {
        let key = format!("{}{}", num_to_col(c.col), c.row);
        let content = c.content.as_ref().unwrap();
        pairs.push((key, cell_dump(content)));
    }
    J::Obj(pairs)
}

fn merges_dump(s: &SheetModel) -> J {
    let mut strs: Vec<String> = s
        .merges
        .iter()
        .map(|m| {
            format!(
                "{}{}:{}{}",
                num_to_col(m.c1),
                m.r1,
                num_to_col(m.c2),
                m.r2
            )
        })
        .collect();
    strs.sort();
    J::Arr(strs.into_iter().map(J::Str).collect())
}

fn tables_dump(s: &SheetModel) -> J {
    let mut entries: Vec<(String, J)> = Vec::new();
    for t in &s.tables {
        let obj = J::Obj(vec![
            ("name".to_string(), J::Str(t.name.clone())),
            (
                "anchor".to_string(),
                J::Str(format!("{}{}", num_to_col(t.anchor.col), t.anchor.row)),
            ),
            (
                "columns".to_string(),
                J::Arr(t.columns.iter().map(|c| J::Str(c.clone())).collect()),
            ),
            ("bodyRows".to_string(), J::Int(t.body_rows)),
            ("hasTotals".to_string(), J::Bool(t.has_totals)),
        ]);
        entries.push((t.name.clone(), obj));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    J::Arr(entries.into_iter().map(|(_, j)| j).collect())
}

fn counts_dump(s: &SheetModel) -> J {
    let c = &s.counts;
    let i = |n: usize| J::Int(n as i64);
    J::Obj(vec![
        ("cf".to_string(), i(c.cf)),
        ("validations".to_string(), i(c.validations)),
        ("notes".to_string(), i(c.notes)),
        ("threads".to_string(), i(c.threads)),
        ("scenarios".to_string(), i(c.scenarios)),
        ("sparklines".to_string(), i(c.sparklines)),
        ("charts".to_string(), i(c.charts)),
        ("pivots".to_string(), i(c.pivots)),
        ("slicers".to_string(), i(c.slicers)),
        ("images".to_string(), i(c.images)),
        ("shapes".to_string(), i(c.shapes)),
        ("hyperlinks".to_string(), i(c.hyperlinks)),
    ])
}

fn sheet_dump(s: &SheetModel) -> J {
    let hidden = match s.meta.get("hidden") {
        Some(Yaml::Bool(true)) => J::Bool(true),
        Some(Yaml::Str(v)) if v == "very" => J::Str("very".to_string()),
        _ => J::Bool(false),
    };
    let freeze = match s.meta.get("freeze") {
        Some(Yaml::Null) | None => J::Null,
        Some(v) => J::Str(v.to_js_string()),
    };
    let protected = s
        .meta
        .get("protect")
        .and_then(|p| p.get("enabled"))
        .map(|e| matches!(e, Yaml::Bool(true)))
        .unwrap_or(false);

    J::Obj(vec![
        ("name".to_string(), J::Str(s.name.clone())),
        ("kind".to_string(), J::Str(s.kind.clone())),
        ("hidden".to_string(), hidden),
        ("freeze".to_string(), freeze),
        ("protected".to_string(), J::Bool(protected)),
        ("cells".to_string(), cells_dump(s)),
        ("merges".to_string(), merges_dump(s)),
        ("tables".to_string(), tables_dump(s)),
        ("counts".to_string(), counts_dump(s)),
    ])
}

/// Produce the canonical dump for a model (trailing newline included).
pub fn dump_model(model: &Model) -> String {
    let fm = &model.fm;
    let date_system = if fm.get("date-system").and_then(|d| d.as_i64()) == Some(1904) {
        1904
    } else {
        1900
    };
    let root = J::Obj(vec![
        ("gridmd".to_string(), fm_str_field(fm.get("gridmd"))),
        ("title".to_string(), fm_str_field(fm.get("title"))),
        ("dateSystem".to_string(), J::Int(date_system)),
        ("names".to_string(), names_dump(fm)),
        (
            "sheets".to_string(),
            J::Arr(model.sheets.iter().map(sheet_dump).collect()),
        ),
    ]);
    let mut out = String::new();
    write_json(&root, 0, &mut out);
    out.push('\n');
    out
}
