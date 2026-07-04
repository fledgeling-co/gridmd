//! Materializes a parsed document into the per-sheet workbook model the dump
//! measures (SPEC §12). Port of the dump-relevant half of `js/src/xlsx/model.js`
//! (cells, merges, tables, feature counts, sheet meta). Formatting patches and
//! the non-native carry/report machinery are omitted — they do not affect the
//! canonical model dump.

use crate::parser::{Block, Document, FenceBlock};
use crate::refs::{num_to_col, parse_cell, parse_target, Cell, Target, TargetKind};
use crate::scalar::{parse_scalar, CachedScalar, Scalar};
use crate::yaml::Yaml;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Counts {
    pub cf: usize,
    pub validations: usize,
    pub notes: usize,
    pub threads: usize,
    pub scenarios: usize,
    pub sparklines: usize,
    pub charts: usize,
    pub pivots: usize,
    pub slicers: usize,
    pub images: usize,
    pub shapes: usize,
    pub hyperlinks: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Content {
    pub rich: Option<String>,
    pub formula: Option<String>,
    pub cached: Option<Scalar>,
    pub array_ref: Option<String>,
    pub scalar: Option<Scalar>,
}

#[derive(Debug, Clone)]
pub struct CellModel {
    pub col: i64,
    pub row: i64,
    pub content: Option<Content>,
}

#[derive(Debug, Clone)]
pub struct TableModel {
    pub name: String,
    pub anchor: Cell,
    pub columns: Vec<String>,
    pub body_rows: i64,
    pub has_totals: bool,
}

pub struct SheetModel {
    pub name: String,
    pub meta: Yaml,
    pub kind: String,
    pub cells: Vec<CellModel>,
    pub index: HashMap<(i64, i64), usize>,
    pub merges: Vec<Target>,
    pub tables: Vec<TableModel>,
    pub counts: Counts,
}

pub struct Model {
    pub fm: Yaml,
    pub sheets: Vec<SheetModel>,
}

fn scalar_content(sc: Scalar) -> Content {
    match sc {
        Scalar::Formula {
            formula, cached, ..
        } => Content {
            formula: Some(formula),
            cached: cached_to_scalar(cached),
            ..Default::default()
        },
        other => Content {
            scalar: Some(other),
            ..Default::default()
        },
    }
}

fn cached_to_scalar(cached: Option<Box<CachedScalar>>) -> Option<Scalar> {
    match cached {
        Some(b) => match *b {
            CachedScalar::Value(s) => Some(s),
            CachedScalar::Invalid(_) => None,
        },
        None => None,
    }
}

/// ECMAScript `String(v)`-ish check for `if (x)` truthiness of a YAML value.
fn truthy(v: &Yaml) -> bool {
    match v {
        Yaml::Null => false,
        Yaml::Bool(b) => *b,
        Yaml::Int(n) => *n != 0,
        Yaml::Real(f) => *f != 0.0,
        Yaml::Str(s) => !s.is_empty(),
        Yaml::Array(_) | Yaml::Hash(_) => true,
    }
}

impl SheetModel {
    fn cell_at(&mut self, col: i64, row: i64) -> usize {
        if let Some(&idx) = self.index.get(&(col, row)) {
            return idx;
        }
        let idx = self.cells.len();
        self.cells.push(CellModel {
            col,
            row,
            content: None,
        });
        self.index.insert((col, row), idx);
        idx
    }

    /// `setContent`: first content wins; a later `{cached}`-only content donates
    /// its cached value into an existing cache-less formula (spill-cache rule).
    fn set_content(&mut self, col: i64, row: i64, content: Content) {
        let idx = self.cell_at(col, row);
        let existing = &mut self.cells[idx].content;
        match existing {
            None => *existing = Some(content),
            Some(cur) => {
                if let Some(donor) = content.cached {
                    if cur.formula.is_some() && cur.cached.is_none() {
                        cur.cached = Some(donor);
                    }
                }
            }
        }
    }
}

fn resolve_style(name: &str, styles: &Yaml, seen: &mut Vec<String>) -> Vec<(String, Yaml)> {
    let def = match styles.get(name) {
        Some(Yaml::Hash(h)) => h,
        _ => return Vec::new(),
    };
    if seen.iter().any(|s| s == name) {
        return Vec::new();
    }
    seen.push(name.to_string());
    let mut out: Vec<(String, Yaml)> = Vec::new();
    if let Some(ext) = def.iter().find(|(k, _)| k.key_str() == "extend") {
        if let Some(ext_name) = ext.1.as_str() {
            out = resolve_style(ext_name, styles, seen);
        }
    }
    for (k, v) in def {
        let ks = k.key_str();
        if ks == "extend" {
            continue;
        }
        overlay(&mut out, ks, v.clone());
    }
    out
}

fn overlay(map: &mut Vec<(String, Yaml)>, key: String, val: Yaml) {
    if let Some(slot) = map.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = val;
    } else {
        map.push((key, val));
    }
}

const CONTENT_ONLY_KEYS: [&str; 7] =
    ["value", "formula", "rich", "entity", "fields", "spill", "array"];

/// The effective annotation patch (`expandPatch({...flow, ...bodyProps})`),
/// used only to extract merge/link/note here.
fn expand_patch(flow: &Yaml, body: &Yaml, styles: &Yaml) -> Vec<(String, Yaml)> {
    let mut combined: Vec<(String, Yaml)> = Vec::new();
    if let Some(h) = flow.as_hash() {
        for (k, v) in h {
            overlay(&mut combined, k.key_str(), v.clone());
        }
    }
    if let Some(h) = body.as_hash() {
        for (k, v) in h {
            let ks = k.key_str();
            if CONTENT_ONLY_KEYS.contains(&ks.as_str()) {
                continue;
            }
            overlay(&mut combined, ks, v.clone());
        }
    }
    let style = combined
        .iter()
        .find(|(k, _)| k == "style")
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()));
    let mut out = match style {
        Some(name) => resolve_style(&name, styles, &mut Vec::new()),
        None => Vec::new(),
    };
    for (k, v) in combined {
        if k == "style" {
            continue;
        }
        overlay(&mut out, k, v);
    }
    out
}

fn get_prop<'a>(map: &'a [(String, Yaml)], key: &str) -> Option<&'a Yaml> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn yaml_scalar(v: &Yaml) -> Scalar {
    match v {
        Yaml::Int(n) => Scalar::Number(*n as f64),
        Yaml::Real(f) => Scalar::Number(*f),
        Yaml::Bool(b) => Scalar::Boolean(*b),
        other => {
            let s = other.to_js_string();
            if crate::scalar::is_date_str(&s) {
                Scalar::Date(s)
            } else if crate::scalar::is_time_str(&s) {
                Scalar::Time(s)
            } else {
                Scalar::Text {
                    value: s,
                    problem: None,
                }
            }
        }
    }
}


pub fn build_model(doc: &Document) -> Model {
    let fm = doc.frontmatter.clone();
    let styles = fm.get("styles").cloned().unwrap_or(Yaml::Null);
    let mut sheets = Vec::new();

    for sheet in &doc.sheets {
        let meta = sheet
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Fence(f) if f.kind == "sheet" => Some(f.meta.clone()),
                _ => None,
            })
            .unwrap_or(Yaml::Hash(Vec::new()));
        let kind = if meta.get("kind").and_then(|k| k.as_str()) == Some("chart") {
            "chart".to_string()
        } else {
            "worksheet".to_string()
        };
        let mut s = SheetModel {
            name: sheet.name.clone(),
            meta,
            kind,
            cells: Vec::new(),
            index: HashMap::new(),
            merges: Vec::new(),
            tables: Vec::new(),
            counts: Counts::default(),
        };

        for b in &sheet.blocks {
            match b {
                Block::At(a) => apply_at(&mut s, a, &styles),
                Block::Fence(f) => apply_fence(&mut s, f),
            }
        }
        sheets.push(s);
    }

    Model { fm, sheets }
}

fn apply_fence(s: &mut SheetModel, f: &FenceBlock) {
    match f.kind.as_str() {
        "sheet" => {}
        "grid" => {
            if let Some(a) = f.args.positional.first().and_then(|p| parse_cell(p)) {
                for (ri, row) in f.rows.iter().enumerate() {
                    for (ci, text) in row.cells.iter().enumerate() {
                        let sc = parse_scalar(text);
                        if !matches!(sc, Scalar::Blank) {
                            s.set_content(a.col + ci as i64, a.row + ri as i64, scalar_content(sc));
                        }
                    }
                }
            }
        }
        "spill-cache" => {
            if let Some(a) = f.args.positional.first().and_then(|p| parse_cell(p)) {
                for (ri, row) in f.rows.iter().enumerate() {
                    for (ci, text) in row.cells.iter().enumerate() {
                        let sc = parse_scalar(text);
                        if matches!(sc, Scalar::Blank) {
                            continue;
                        }
                        if ri == 0 && ci == 0 {
                            s.set_content(
                                a.col,
                                a.row,
                                Content {
                                    cached: Some(sc),
                                    ..Default::default()
                                },
                            );
                        } else {
                            s.set_content(
                                a.col + ci as i64,
                                a.row + ri as i64,
                                Content {
                                    scalar: Some(sc),
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }
            }
        }
        "table" => apply_table(s, f),
        "cf" => {
            s.counts.cf += f.meta.as_array().map(|a| a.len()).unwrap_or(0);
        }
        "validation" => s.counts.validations += 1,
        "comments" => s.counts.threads += 1,
        "scenario" => s.counts.scenarios += 1,
        "sparklines" => s.counts.sparklines += 1,
        "chart" => s.counts.charts += 1,
        "pivot" => s.counts.pivots += 1,
        "slicer" => s.counts.slicers += 1,
        "image" => s.counts.images += 1,
        "shape" => s.counts.shapes += 1,
        "textbox" => s.counts.shapes += 1,
        _ => {}
    }
}

fn apply_table(s: &mut SheetModel, f: &FenceBlock) {
    let anchor = match f.args.anchor.as_deref().and_then(parse_cell) {
        Some(a) => a,
        None => return,
    };
    let tm = &f.meta;
    let header = tm.get("header").and_then(|h| h.as_bool()) != Some(false);
    let mut columns: Vec<String> = Vec::new();
    for (ri, row) in f.rows.iter().enumerate() {
        for (ci, text) in row.cells.iter().enumerate() {
            let sc = parse_scalar(text);
            if header && ri == 0 {
                if let Scalar::Text { value, .. } = &sc {
                    columns.push(value.clone());
                }
            }
            if !matches!(sc, Scalar::Blank) {
                s.set_content(
                    anchor.col + ci as i64,
                    anchor.row + ri as i64,
                    scalar_content(sc),
                );
            }
        }
    }
    let has_totals = tm.get("total").is_some();
    if let Some(Yaml::Hash(total)) = tm.get("total") {
        let total_row = anchor.row + f.rows.len() as i64;
        for (k, v) in total {
            let col_name = k.key_str();
            let ci = columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&col_name));
            if let Some(ci) = ci {
                let sc = parse_scalar(&v.to_js_string());
                s.set_content(anchor.col + ci as i64, total_row, scalar_content(sc));
            }
        }
    }
    let body_rows = f.rows.len() as i64 - if header { 1 } else { 0 };
    s.tables.push(TableModel {
        name: f.args.positional.first().cloned().unwrap_or_default(),
        anchor,
        columns,
        body_rows,
        has_totals,
    });
}

fn apply_at(s: &mut SheetModel, a: &crate::parser::AtBlock, styles: &Yaml) {
    let t = match parse_target(&a.target_text) {
        Some(t) => t,
        None => return,
    };
    let body = a.body.clone().unwrap_or(Yaml::Hash(Vec::new()));
    let flow = a.props.clone().unwrap_or(Yaml::Hash(Vec::new()));

    if let Some(scalar_text) = &a.scalar_text {
        let sc = parse_scalar(scalar_text);
        if t.kind == TargetKind::Cell && !matches!(sc, Scalar::Blank) {
            let mut content = scalar_content(sc);
            if content.formula.is_some() {
                let spill = flow.get("spill").or_else(|| body.get("spill"));
                let arr = flow.get("array").or_else(|| body.get("array"));
                if let Some(v) = spill.or(arr) {
                    content.array_ref = Some(v.to_js_string());
                }
                // cse flag is not part of the dump; skip tracking.
            }
            s.set_content(t.c1, t.r1, content);
        } else if t.kind == TargetKind::Range {
            if let Scalar::Formula { formula, .. } = &sc {
                for r in t.r1..=t.r2 {
                    for c in t.c1..=t.c2 {
                        let translated = translate_formula(formula, r - t.r1, c - t.c1);
                        s.set_content(
                            c,
                            r,
                            Content {
                                formula: Some(translated),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }
    } else if let Some(content) = body_content(&body, &flow) {
        if t.kind == TargetKind::Cell {
            s.set_content(t.c1, t.r1, content);
        }
    }

    let patch = expand_patch(&flow, &body, styles);
    let merge = get_prop(&patch, "merge");
    let link = get_prop(&patch, "link");
    let note = get_prop(&patch, "note").or_else(|| body.get("note"));

    if merge == Some(&Yaml::Bool(true)) && t.kind == TargetKind::Range {
        s.merges.push(t.clone());
    }
    if link.map(truthy).unwrap_or(false) {
        s.counts.hyperlinks += 1;
    }
    if note.map(truthy).unwrap_or(false) {
        s.counts.notes += 1;
    }
}

fn body_content(body: &Yaml, flow: &Yaml) -> Option<Content> {
    if let Some(f) = body.get("formula") {
        let formula = f.to_js_string();
        let formula = formula.strip_prefix('=').unwrap_or(&formula).to_string();
        let cached = body.get("value").map(yaml_scalar);
        let mut content = Content {
            formula: Some(formula),
            cached,
            ..Default::default()
        };
        let spill = body.get("spill").or_else(|| flow.get("spill"));
        let arr = body.get("array").or_else(|| flow.get("array"));
        if let Some(v) = spill.or(arr) {
            content.array_ref = Some(v.to_js_string());
        }
        return Some(content);
    }
    if let Some(rich) = body.get("rich") {
        let text = rich_text(rich);
        return Some(Content {
            rich: Some(text),
            ..Default::default()
        });
    }
    if let Some(entity) = body.get("entity") {
        let text = entity
            .get("text")
            .or_else(|| entity.get("id"))
            .map(|v| v.to_js_string())
            .unwrap_or_default();
        return Some(Content {
            scalar: Some(Scalar::Text {
                value: text,
                problem: None,
            }),
            ..Default::default()
        });
    }
    if let Some(value) = body.get("value") {
        return Some(Content {
            scalar: Some(yaml_scalar(value)),
            ..Default::default()
        });
    }
    None
}

/// Concatenate rich-run `text` fields (JS `rich.map(r => r.text).join('')`,
/// where a missing `text` contributes the empty string).
fn rich_text(rich: &Yaml) -> String {
    let mut out = String::new();
    if let Some(runs) = rich.as_array() {
        for run in runs {
            if let Some(t) = run.get("text") {
                out.push_str(&t.to_js_string());
            }
        }
    }
    out
}

/// Relative fill (SPEC §8.5): shift unanchored A1 refs by `(dr, dc)`, skipping
/// string literals and quoted sheet names. Port of `translateFormula`.
pub fn translate_formula(formula: &str, dr: i64, dc: i64) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            let q = ch;
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == q {
                    if j + 1 < chars.len() && chars[j + 1] == q {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(chars.len());
            out.extend(&chars[i..end]);
            i = end;
            continue;
        }
        let prev = out.chars().last();
        if let Some((whole, cd, col_l, rd, row_s)) = match_a1(&chars[i..]) {
            let prev_ok = prev.map(|p| !(p.is_ascii_alphanumeric() || p == '_' || p == '.')).unwrap_or(true);
            if prev_ok {
                let col = if cd == "$" {
                    col_l.clone()
                } else {
                    num_to_col((crate::refs::col_to_num(&col_l) + dc).max(1))
                };
                let row = if rd == "$" {
                    row_s.clone()
                } else {
                    (row_s.parse::<i64>().unwrap_or(1) + dr).max(1).to_string()
                };
                out.push_str(&cd);
                out.push_str(&col);
                out.push_str(&rd);
                out.push_str(&row);
                i += whole;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// Match `^(\$?)([A-Z]{1,3})(\$?)(\d{1,7})(?![A-Za-z0-9_(])` at the slice start.
/// Returns `(char_len, col$, colLetters, row$, rowDigits)`.
fn match_a1(chars: &[char]) -> Option<(usize, String, String, String, String)> {
    let mut i = 0;
    let cd = if i < chars.len() && chars[i] == '$' {
        i += 1;
        "$"
    } else {
        ""
    };
    let col_start = i;
    while i < chars.len() && chars[i].is_ascii_uppercase() && i - col_start < 3 {
        i += 1;
    }
    if i == col_start {
        return None;
    }
    let col: String = chars[col_start..i].iter().collect();
    let rd = if i < chars.len() && chars[i] == '$' {
        i += 1;
        "$"
    } else {
        ""
    };
    let row_start = i;
    while i < chars.len() && chars[i].is_ascii_digit() && i - row_start < 7 {
        i += 1;
    }
    if i == row_start {
        return None;
    }
    let row: String = chars[row_start..i].iter().collect();
    // negative lookahead (?![A-Za-z0-9_(])
    if let Some(&next) = chars.get(i) {
        if next.is_ascii_alphanumeric() || next == '_' || next == '(' {
            return None;
        }
    }
    Some((i, cd.to_string(), col, rd.to_string(), row))
}
