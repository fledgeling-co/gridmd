//! GridMD semantic validation (SPEC §9.4, §12–§13). Port of `js/src/validate.js`.
//! Strict-mode rejection is the conformance contract (Law 2); the valid fixtures
//! must validate with zero errors.

use crate::diag::Diag;
use crate::parser::{is_reserved_kind, AtBlock, Block, Document, FenceBlock, Stats};
use crate::refs::{parse_cell, parse_target, Target, TargetKind, MAX_COL, MAX_ROW};
use crate::scalar::{parse_scalar, CachedScalar, Scalar};
use crate::yaml::Yaml;
use std::collections::HashMap;

const WORKBOOK_KINDS: &[&str] = &["query", "script", "raw"];
const CONTENT_KEYS: &[&str] = &["value", "formula", "rich", "entity"];
const FILL_ENUMERATION_CAP: i64 = 10000;

const KNOWN_PROPS: &[&str] = &[
    "style", "font", "size", "bold", "italic", "underline", "strike", "sub",
    "super", "color", "fill", "pattern", "fill2", "border", "border-top",
    "border-right", "border-bottom", "border-left", "border-diag-up",
    "border-diag-down", "border-inner", "border-inner-h", "border-inner-v",
    "align", "valign", "rotation", "indent", "wrap", "shrink", "numfmt",
    "merge", "locked", "hidden", "link", "tip", "note", "rich", "spill",
    "array", "control", "entity", "fields", "value", "formula",
];

const SHEET_META_KEYS: &[&str] = &[
    "kind", "tab-color", "hidden", "freeze", "split", "view",
    "default-row-height", "default-col-width", "cols", "rows", "protect", "names",
];

const FRONTMATTER_KEYS: &[&str] = &[
    "gridmd", "title", "properties", "locale", "date-system", "calc", "theme",
    "names", "styles", "table-styles", "links", "protection",
];

const CHART_TYPES: &[&str] = &[
    "column", "bar", "line", "area", "pie", "doughnut", "scatter", "bubble",
    "radar", "stock", "surface", "histogram", "pareto", "box-whisker",
    "treemap", "sunburst", "waterfall", "funnel", "map", "combo",
];

const SHAPE_KINDS: &[&str] = &[
    "rect", "rounded-rect", "ellipse", "triangle", "right-triangle", "diamond",
    "pentagon", "hexagon", "star", "arrow-right", "arrow-left", "arrow-up",
    "arrow-down", "chevron", "callout", "line", "connector",
];

const VALIDATION_TYPES: &[&str] = &["list", "whole", "decimal", "date", "time", "text-length", "custom"];
const CF_RULE_KEYS: &[&str] = &[
    "when", "contains", "not-contains", "begins", "ends", "date", "dupes",
    "unique", "top", "bottom", "avg", "bars", "scale", "icons", "formula",
];

fn chart_base_type(t: &str) -> &str {
    let mut base = t;
    for suf in ["-stacked100", "-stacked", "-3d"] {
        if let Some(stripped) = base.strip_suffix(suf) {
            base = stripped;
        }
    }
    base
}

fn is_hex_color(s: &str) -> bool {
    // ^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$
    let b = s.as_bytes();
    if b.first() != Some(&b'#') {
        return false;
    }
    let hex = &s[1..];
    (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|c| c.is_ascii_hexdigit())
}

fn is_theme_color(s: &str) -> bool {
    // ^(dk1|lt1|dk2|lt2|accent[1-6]|hlink|folHlink)(@-?\d{1,3})?$
    let (slot, tint) = match s.split_once('@') {
        Some((slot, t)) => (slot, Some(t)),
        None => (s, None),
    };
    if !is_theme_slot(slot) {
        return false;
    }
    match tint {
        None => true,
        Some(t) => {
            let digits = t.strip_prefix('-').unwrap_or(t);
            !digits.is_empty() && digits.len() <= 3 && digits.bytes().all(|c| c.is_ascii_digit())
        }
    }
}

fn is_theme_slot(s: &str) -> bool {
    matches!(s, "dk1" | "lt1" | "dk2" | "lt2" | "hlink" | "folHlink")
        || (s.starts_with("accent")
            && s.len() == 7
            && matches!(s.as_bytes()[6], b'1'..=b'6'))
}

fn is_color(v: &Yaml) -> bool {
    match v.as_str() {
        Some(s) => s == "auto" || is_hex_color(s) || is_theme_color(s),
        None => false,
    }
}

fn is_safe_link(v: &Yaml) -> bool {
    match v.as_str() {
        Some(s) => s.starts_with("https://") || s.starts_with("mailto:") || s.starts_with('#'),
        None => false,
    }
}

fn is_safe_image_src(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("vbscript:") || lower.starts_with("file:")
    {
        return false;
    }
    if lower.starts_with("data:") {
        return lower.starts_with("data:image/");
    }
    // ^[a-z][a-z0-9+.-]*:  → any scheme; only https allowed
    if let Some(colon) = v.find(':') {
        let scheme = &v[..colon];
        let sb = scheme.as_bytes();
        if !sb.is_empty()
            && sb[0].is_ascii_alphabetic()
            && sb.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'+' || c == b'.' || c == b'-')
        {
            return lower.starts_with("https:");
        }
    }
    true // relative path
}

/// `{raw}` `part=` path rules (DIRECTIVES §18).
pub fn is_valid_part_path(p: &str) -> bool {
    if p.is_empty() {
        return false;
    }
    if p.starts_with('/') || p.contains('\\') {
        return false;
    }
    if p.bytes().any(|c| c < 0x20 || c == b' ') {
        return false;
    }
    let low = p.to_ascii_lowercase();
    if low.contains("%2e") || low.contains("%2f") || low.contains("%5c") {
        return false;
    }
    p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

struct SpillRange {
    c1: i64,
    r1: i64,
    c2: i64,
    r2: i64,
}

struct SheetState {
    name: String,
    defs: HashMap<(i64, i64), usize>,
    spills: Vec<SpillRange>,
}

pub struct Validator {
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
    pub stats: Stats,
    global_names: HashMap<String, &'static str>,
}

impl Validator {
    fn err(&mut self, line: usize, msg: impl Into<String>) {
        self.errors.push(Diag::new(line, msg));
    }
    fn warn(&mut self, line: usize, msg: impl Into<String>) {
        self.warnings.push(Diag::new(line, msg));
    }
}

fn add_def(
    v: &mut Validator,
    sheet: &mut SheetState,
    col: i64,
    row: i64,
    line: usize,
    what: &str,
) {
    if col > MAX_COL || row > MAX_ROW {
        v.err(line, format!("{what}: cell out of bounds"));
        return;
    }
    if let Some(prev) = sheet.defs.get(&(col, row)) {
        v.err(
            line,
            format!("{what}: cell defined more than once (previous definition at line {prev})"),
        );
        return;
    }
    sheet.defs.insert((col, row), line);
    v.stats.defs += 1;
}

/// ctx.target — resolve + kind-check a directive target within a sheet.
fn target_of(
    v: &mut Validator,
    sheet_name: &str,
    text: Option<&str>,
    line: usize,
    kinds: &[TargetKind],
    what: &str,
) -> Option<Target> {
    let t = parse_target(text.unwrap_or(""));
    match &t {
        Some(tt) if kinds.contains(&tt.kind) => {
            if let Some(sh) = &tt.sheet {
                if sh.to_lowercase() != sheet_name.to_lowercase() {
                    v.err(
                        line,
                        format!(
                            "{what}: anchor qualifier {sh}! must name the containing sheet ({sheet_name})"
                        ),
                    );
                }
            }
            t
        }
        _ => {
            v.err(
                line,
                format!("{what}: invalid target {}", text.unwrap_or("")),
            );
            None
        }
    }
}

pub fn validate_document(doc: &Document) -> Validator {
    let mut v = Validator {
        errors: Vec::new(),
        warnings: Vec::new(),
        stats: Stats::default(),
        global_names: HashMap::new(),
    };

    validate_frontmatter(&mut v, doc);

    // ---- workbook-level blocks ----
    for b in &doc.workbook_blocks {
        v.stats.blocks += 1;
        match b {
            Block::At(a) => {
                v.err(a.line, "@ directives are not allowed before the first sheet");
            }
            Block::Fence(f) => {
                if f.kind.starts_with("x-") {
                    continue;
                }
                if !is_reserved_kind(&f.kind) {
                    v.err(f.line, format!("unknown directive {{{}}}", f.kind));
                    continue;
                }
                if !WORKBOOK_KINDS.contains(&f.kind.as_str()) {
                    v.err(
                        f.line,
                        format!(
                            "{{{}}} is sheet-scoped and cannot appear before the first sheet",
                            f.kind
                        ),
                    );
                    continue;
                }
                validate_fence(&mut v, f, None);
            }
        }
    }

    // ---- sheets ----
    if doc.sheets.is_empty() {
        v.err(1, "a workbook requires at least one sheet (a level-1 heading)");
    }
    let mut sheet_names: HashMap<String, ()> = HashMap::new();
    for sheet in &doc.sheets {
        let name_key = sheet.name.to_lowercase();
        if sheet.name.chars().count() > 31 {
            v.err(sheet.line, format!("sheet name exceeds 31 chars: {}", sheet.name));
        }
        if sheet.name.contains([':', '\\', '/', '?', '*', '[', ']']) {
            v.err(
                sheet.line,
                format!(
                    "sheet name contains a forbidden character (: \\ / ? * [ ]): {}",
                    sheet.name
                ),
            );
        }
        if sheet_names.contains_key(&name_key) {
            v.err(sheet.line, format!("duplicate sheet name: {}", sheet.name));
        }
        sheet_names.insert(name_key, ());
        validate_sheet(&mut v, sheet);
    }

    v
}

fn validate_frontmatter(v: &mut Validator, doc: &Document) {
    let fm = &doc.frontmatter;
    let gridmd_ok = fm
        .get("gridmd")
        .and_then(|g| g.as_str())
        .map(|s| {
            let mut parts = s.splitn(2, '.');
            match (parts.next(), parts.next()) {
                (Some(a), Some(b)) => {
                    !a.is_empty()
                        && a.bytes().all(|c| c.is_ascii_digit())
                        && !b.is_empty()
                        && b.bytes().all(|c| c.is_ascii_digit())
                }
                _ => false,
            }
        })
        .unwrap_or(false);
    if !gridmd_ok {
        v.err(2, "frontmatter requires gridmd: \"MAJOR.MINOR\" (quoted string)");
    }
    if let Some(Yaml::Hash(pairs)) = Some(fm) {
        for (k, _) in pairs {
            let key = k.key_str();
            if !FRONTMATTER_KEYS.contains(&key.as_str()) && !key.starts_with("x-") {
                v.warn(2, format!("unknown frontmatter key: {key}"));
            }
        }
    }
    if let Some(ds) = fm.get("date-system") {
        if ds.as_i64() != Some(1900) && ds.as_i64() != Some(1904) {
            v.err(2, "date-system must be 1900 or 1904");
        }
    }
    if let Some(mode) = fm.get("calc").and_then(|c| c.get("mode")) {
        if let Some(m) = mode.as_str() {
            if !["auto", "auto-no-tables", "manual"].contains(&m) {
                v.err(2, format!("calc.mode must be auto | auto-no-tables | manual, got {m}"));
            }
        }
    }
    if let Some(names) = fm.get("names").and_then(|n| n.as_array()) {
        for n in names {
            let name = n.get("name").and_then(|x| x.as_str());
            match name {
                None => {
                    v.err(2, "names entries require a name");
                    continue;
                }
                Some(name) => {
                    let forms = ["ref", "formula", "value"]
                        .iter()
                        .filter(|k| n.get(k).is_some())
                        .count();
                    if forms != 1 {
                        v.err(2, format!("name {name}: exactly one of ref | formula | value required"));
                    }
                    let key = name.to_lowercase();
                    if v.global_names.contains_key(&key) {
                        v.err(2, format!("duplicate defined name: {name}"));
                    }
                    v.global_names.insert(key, "name");
                }
            }
        }
    }
    if let Some(Yaml::Hash(styles)) = fm.get("styles") {
        for (name, style) in styles {
            if !matches!(style, Yaml::Hash(_)) {
                v.err(2, format!("style {} must be a mapping", name.key_str()));
            }
        }
    }
    if let Some(Yaml::Hash(colors)) = fm.get("theme").and_then(|t| t.get("colors")) {
        for (slot, val) in colors {
            let slot = slot.key_str();
            if !is_theme_slot(&slot) {
                v.warn(2, format!("unknown theme color slot: {slot}"));
            } else if !is_hex_color(&val.to_js_string()) {
                v.err(2, format!("theme color {slot} must be #RRGGBB"));
            }
        }
    }
}

fn validate_sheet(v: &mut Validator, sheet: &crate::parser::Sheet) {
    let mut state = SheetState {
        name: sheet.name.clone(),
        defs: HashMap::new(),
        spills: Vec::new(),
    };
    let mut sheet_meta_lines: Vec<usize> = Vec::new();
    let mut sheet_meta: Option<&FenceBlock> = None;
    let mut charts_at_sheet = 0usize;
    let mut grid_content = 0usize;
    let mut spill_caches: Vec<&FenceBlock> = Vec::new();

    for b in &sheet.blocks {
        v.stats.blocks += 1;
        match b {
            Block::At(a) => {
                validate_at(v, &mut state, a);
            }
            Block::Fence(f) => {
                if f.kind.starts_with("x-") {
                    continue;
                }
                if !is_reserved_kind(&f.kind) {
                    v.err(f.line, format!("unknown directive {{{}}}", f.kind));
                    continue;
                }
                if f.kind == "sheet" {
                    if sheet_meta.is_none() {
                        sheet_meta = Some(f);
                    }
                    sheet_meta_lines.push(f.line);
                    validate_sheet_meta(v, f);
                    continue;
                }
                if f.kind == "grid" || f.kind == "table" {
                    grid_content += 1;
                }
                if f.kind == "spill-cache" {
                    spill_caches.push(f);
                    continue;
                }
                if f.kind == "chart" && f.args.anchor.as_deref() == Some("sheet") {
                    charts_at_sheet += 1;
                }
                validate_fence(v, f, Some(&mut state));
            }
        }
    }

    if sheet_meta_lines.len() > 1 {
        v.err(sheet_meta_lines[1], "multiple {sheet} blocks in one sheet");
    }
    if let Some(first_meta) = sheet_meta {
        let is_first = matches!(sheet.blocks.first(), Some(Block::Fence(f)) if std::ptr::eq(f, first_meta));
        if !is_first {
            v.warn(first_meta.line, "{sheet} should be the first block of its sheet");
        }
    }
    let meta = sheet_meta.map(|f| &f.meta);
    let kind_is_chart = meta
        .and_then(|m| m.get("kind"))
        .and_then(|k| k.as_str())
        == Some("chart");

    if kind_is_chart {
        if charts_at_sheet != 1 {
            v.err(
                sheet.line,
                format!(
                    "a chart sheet requires exactly one {{chart}} anchored `at sheet` (found {charts_at_sheet})"
                ),
            );
        }
        if grid_content > 0 || !state.defs.is_empty() {
            v.err(sheet.line, "a chart sheet cannot carry worksheet grid content");
        }
    } else if charts_at_sheet > 0 {
        v.err(sheet.line, "`at sheet` chart anchors require {sheet} kind: chart");
    }

    for sc in spill_caches {
        let anchor = sc.args.positional.first().and_then(|p| parse_cell(p));
        let anchor = match anchor {
            Some(a) => a,
            None => {
                v.err(sc.line, "{spill-cache} requires a cell anchor");
                continue;
            }
        };
        let h = sc.rows.len() as i64;
        let w = sc.rows.iter().map(|r| r.cells.len() as i64).max().unwrap_or(0);
        let owner = state
            .spills
            .iter()
            .find(|s| s.c1 == anchor.col && s.r1 == anchor.row);
        match owner {
            None => {
                v.err(
                    sc.line,
                    format!(
                        "{{spill-cache}} at {} has no owning spill/array formula at that anchor",
                        sc.args.positional.first().map(|s| s.as_str()).unwrap_or("")
                    ),
                );
            }
            Some(owner) => {
                if anchor.row + h - 1 > owner.r2 || anchor.col + w - 1 > owner.c2 {
                    v.err(sc.line, "{spill-cache} rectangle exceeds the declared spill/array range");
                }
            }
        }
    }
}

fn validate_at(v: &mut Validator, state: &mut SheetState, b: &AtBlock) {
    let t = match parse_target(&b.target_text) {
        Some(t) => t,
        None => {
            v.err(b.line, format!("invalid @ target: {}", b.target_text));
            return;
        }
    };
    if let Some(sh) = &t.sheet {
        if sh.to_lowercase() != state.name.to_lowercase() {
            v.err(b.line, format!("@ target qualifier {sh}! must name the containing sheet"));
        }
    }
    let empty = Yaml::Hash(Vec::new());
    let body = b.body.as_ref().unwrap_or(&empty);
    let flow = b.props.as_ref().unwrap_or(&empty);
    // props = {...flow, ...body}
    let mut props: Vec<(String, Yaml)> = Vec::new();
    if let Some(h) = flow.as_hash() {
        for (k, val) in h {
            overlay(&mut props, k.key_str(), val.clone());
        }
    }
    if let Some(h) = body.as_hash() {
        for (k, val) in h {
            overlay(&mut props, k.key_str(), val.clone());
        }
    }

    let body_content_keys: Vec<&str> = CONTENT_KEYS
        .iter()
        .copied()
        .filter(|k| body.get(k).is_some())
        .collect();
    let mut scalar: Option<Scalar> = None;
    if let Some(text) = &b.scalar_text {
        let sc = parse_scalar(text);
        if let Scalar::Text {
            problem: Some(p), ..
        } = &sc
        {
            v.err(b.line, format!("scalar: {p}"));
        }
        if let Scalar::Formula { cached, .. } = &sc {
            if let Some(c) = cached {
                if let CachedScalar::Invalid(p) = c.as_ref() {
                    v.err(b.line, format!("scalar: {p}"));
                }
            }
        }
        let cached_only = body_content_keys.len() == 1
            && body_content_keys[0] == "value"
            && matches!(sc, Scalar::Formula { .. });
        if !body_content_keys.is_empty() && !cached_only {
            v.err(b.line, "inline content and body content keys on the same @ directive");
        }
        scalar = Some(sc);
    }
    let has_formula = matches!(scalar, Some(Scalar::Formula { .. })) || body.get("formula").is_some();
    let has_content = matches!(&scalar, Some(s) if !matches!(s, Scalar::Blank))
        || !body_content_keys.is_empty();

    if has_content {
        if t.kind == TargetKind::Cell {
            add_def(v, state, t.c1, t.r1, b.line, "@");
        } else if t.kind == TargetKind::Range && has_formula {
            let count = (t.r2 - t.r1 + 1) * (t.c2 - t.c1 + 1);
            if count > FILL_ENUMERATION_CAP {
                v.warn(b.line, format!("relative fill over {count} cells — overlap checking skipped"));
            } else {
                for r in t.r1..=t.r2 {
                    for c in t.c1..=t.c2 {
                        add_def(v, state, c, r, b.line, "@ fill");
                    }
                }
            }
        } else {
            v.err(b.line, "range targets accept formula content only (relative fill, SPEC §8.5/§9.4)");
        }
    }

    for (k, val) in &props {
        if !KNOWN_PROPS.contains(&k.as_str()) && !k.starts_with("x-") {
            v.warn(b.line, format!("unknown property: {k}"));
        }
        if (k == "fill" || k == "color") && !is_color(val) {
            v.err(b.line, format!("{k}: not a color: {}", val.to_js_string()));
        }
        if k == "link" && !is_safe_link(val) {
            v.err(b.line, format!("link: scheme must be https:, mailto:, or internal #: {}", val.to_js_string()));
        }
        if k == "merge" {
            if t.kind != TargetKind::Range {
                v.err(b.line, "merge: requires a range target");
            }
            if *val != Yaml::Bool(true) {
                v.err(b.line, "merge: only `true` is valid");
            }
        }
        if k == "spill" || k == "array" {
            let st = parse_target(&val.to_js_string());
            match st {
                Some(st) if st.kind == TargetKind::Range => {
                    if t.kind != TargetKind::Cell || st.c1 != t.c1 || st.r1 != t.r1 {
                        v.err(b.line, format!("{k}: range must start at the anchor cell"));
                    }
                    state.spills.push(SpillRange {
                        c1: st.c1,
                        r1: st.r1,
                        c2: st.c2,
                        r2: st.r2,
                    });
                }
                _ => {
                    v.err(b.line, format!("{k}: must be a range"));
                }
            }
        }
        if k == "rich" && !val.is_array() {
            v.err(b.line, "rich: must be a list of runs");
        }
        if k == "control" && val.as_str() != Some("checkbox") {
            v.err(b.line, format!("control: unknown control {}", val.to_js_string()));
        }
    }
    if body.get("formula").is_some() && body.get("value").is_none() {
        v.warn(b.line, "formula without a cached value: readers will need a calc engine to display");
    }
}

fn overlay(map: &mut Vec<(String, Yaml)>, key: String, val: Yaml) {
    if let Some(slot) = map.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = val;
    } else {
        map.push((key, val));
    }
}

fn validate_sheet_meta(v: &mut Validator, b: &FenceBlock) {
    let m = &b.meta;
    if let Some(pairs) = m.as_hash() {
        for (k, _) in pairs {
            let key = k.key_str();
            if !SHEET_META_KEYS.contains(&key.as_str()) && !key.starts_with("x-") {
                v.warn(b.line, format!("unknown {{sheet}} key: {key}"));
            }
        }
    }
    if let Some(kind) = m.get("kind").and_then(|k| k.as_str()) {
        if kind != "worksheet" && kind != "chart" {
            v.err(b.line, "{sheet} kind must be worksheet | chart");
        }
    }
    if let Some(tc) = m.get("tab-color") {
        if !is_color(tc) {
            v.err(b.line, format!("tab-color: not a color: {}", tc.to_js_string()));
        }
    }
    if let Some(h) = m.get("hidden") {
        let ok = matches!(h, Yaml::Bool(_)) || h.as_str() == Some("very");
        if !ok {
            v.err(b.line, "hidden must be false | true | very");
        }
    }
    for key in ["freeze", "split"] {
        if let Some(val) = m.get(key) {
            if parse_cell(&val.to_js_string()).is_none() {
                v.err(b.line, format!("{key}: must be a cell reference"));
            }
        }
    }
    if let Some(Yaml::Hash(cols)) = m.get("cols") {
        for (k, val) in cols {
            let key = k.key_str();
            if !is_col_or_col_range(&key) {
                v.err(b.line, format!("cols key must be a column or column range: {key}"));
            }
            let ok = matches!(val, Yaml::Int(_) | Yaml::Real(_) | Yaml::Hash(_));
            if !ok {
                v.err(b.line, format!("cols.{key}: must be a width or a mapping"));
            }
        }
    }
    if let Some(Yaml::Hash(rows)) = m.get("rows") {
        for (k, _) in rows {
            let key = k.key_str();
            if !is_row_or_row_range(&key) {
                v.err(b.line, format!("rows key must be a row or row range: {key}"));
            }
        }
    }
}

fn is_col_or_col_range(k: &str) -> bool {
    // ^[A-Z]{1,3}(:[A-Z]{1,3})?$
    let one = |s: &str| !s.is_empty() && s.len() <= 3 && s.bytes().all(|c| c.is_ascii_uppercase());
    match k.split_once(':') {
        Some((a, b)) => one(a) && one(b),
        None => one(k),
    }
}

fn is_row_or_row_range(k: &str) -> bool {
    // ^\d+(:\d+)?$
    let one = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    match k.split_once(':') {
        Some((a, b)) => one(a) && one(b),
        None => one(k),
    }
}

fn validate_fence(v: &mut Validator, b: &FenceBlock, mut sheet: Option<&mut SheetState>) {
    let meta = &b.meta;
    let pos = &b.args.positional;
    let sheet_name = sheet.as_ref().map(|s| s.name.clone()).unwrap_or_default();
    let need = |v: &mut Validator, cond: bool, msg: &str| {
        if !cond {
            v.err(b.line, format!("{{{}}} {}", b.kind, msg));
        }
    };

    match b.kind.as_str() {
        "grid" => {
            let anchor = pos.first().and_then(|p| parse_cell(p));
            need(v, anchor.is_some(), "requires a cell anchor");
            if let (Some(anchor), Some(st)) = (anchor, sheet.as_deref_mut()) {
                for (ri, row) in b.rows.iter().enumerate() {
                    for (ci, cell_text) in row.cells.iter().enumerate() {
                        let s = parse_scalar(cell_text);
                        if let Scalar::Text { problem: Some(p), .. } = &s {
                            v.err(row.line, format!("grid cell: {p}"));
                        }
                        if !matches!(s, Scalar::Blank) {
                            add_def(v, st, anchor.col + ci as i64, anchor.row + ri as i64, row.line, "{grid}");
                        }
                    }
                }
            }
        }
        "table" => validate_table(v, b, sheet.as_deref_mut()),
        "cf" => {
            let tgt = target_of(v, &sheet_name, pos.first().map(|s| s.as_str()), b.line, &[TargetKind::Cell, TargetKind::Range, TargetKind::Cols, TargetKind::Rows], "{cf}");
            need(v, tgt.is_some(), "requires a target range");
            let rules = meta.as_array();
            need(v, rules.is_some(), "body must be a YAML list of rules");
            if let Some(rules) = rules {
                for rule in rules {
                    let kinds = CF_RULE_KEYS.iter().filter(|k| rule.get(k).is_some()).count();
                    if kinds != 1 {
                        v.err(b.line, "each cf rule needs exactly one distinguishing key");
                    }
                    if let Some(pri) = rule.get("priority") {
                        let bad = match pri.as_i64() {
                            Some(n) => n < 1,
                            None => true,
                        };
                        if bad {
                            v.err(b.line, "cf priority must be a positive integer");
                        }
                    }
                    for key in ["fill", "color"] {
                        if let Some(fmt) = rule.get("format").and_then(|f| f.get(key)) {
                            if !is_color(fmt) {
                                v.err(b.line, format!("cf format.{key}: not a color: {}", fmt.to_js_string()));
                            }
                        }
                    }
                }
            }
        }
        "validation" => {
            let tgt = target_of(v, &sheet_name, pos.first().map(|s| s.as_str()), b.line, &[TargetKind::Cell, TargetKind::Range, TargetKind::Cols, TargetKind::Rows], "{validation}");
            need(v, tgt.is_some(), "requires a target");
            let ty = meta.get("type").and_then(|t| t.as_str());
            need(v, ty.map(|t| VALIDATION_TYPES.contains(&t)).unwrap_or(false), &format!("type must be one of {}", VALIDATION_TYPES.join(" | ")));
            if ty == Some("list") {
                need(v, meta.get("values").is_some() || meta.get("source").is_some(), "list validation requires values: or source:");
            }
            if let Some(style) = meta.get("error").and_then(|e| e.get("style")).and_then(|s| s.as_str()) {
                need(v, ["stop", "warning", "information"].contains(&style), "error.style must be stop | warning | information");
            }
        }
        "filter" => {
            let tgt = target_of(v, &sheet_name, pos.first().map(|s| s.as_str()), b.line, &[TargetKind::Range], "{filter}");
            need(v, tgt.is_some(), "requires a range");
            if let Some(Yaml::Hash(cols)) = meta.get("cols") {
                for (k, _) in cols {
                    let key = k.key_str();
                    if !is_col_letters(&key) {
                        v.err(b.line, format!("filter cols keys are column letters on plain ranges: {key}"));
                    }
                }
            }
        }
        "chart" => {
            if let Some(ty) = pos.first() {
                if !CHART_TYPES.contains(&chart_base_type(ty)) {
                    v.warn(b.line, format!("unknown chart type {ty} — a converter must carry it via fallback:"));
                }
            }
            need(v, b.args.anchor.is_some(), "requires `at <anchor>` (or `at sheet` on a chart sheet)");
            if let Some(anchor) = &b.args.anchor {
                if anchor != "sheet" {
                    target_of(v, &sheet_name, Some(anchor), b.line, &[TargetKind::Cell, TargetKind::Range], "{chart} at");
                }
            }
            need(v, meta.get("series").is_some() || meta.get("data").is_some() || meta.get("pivot").is_some(), "requires series:, data:, or pivot:");
            if let Some(series) = meta.get("series").and_then(|s| s.as_array()) {
                for (i, s) in series.iter().enumerate() {
                    let has_val = s.get("val").is_some();
                    if !has_val && meta.get("pivot").is_none() {
                        v.err(b.line, format!("series[{i}] requires val:"));
                    }
                    if let Some(color) = s.get("color") {
                        if !is_color(color) {
                            v.err(b.line, format!("series[{i}].color: not a color"));
                        }
                    }
                }
            }
        }
        "sparklines" => {
            let tgt = target_of(v, &sheet_name, pos.first().map(|s| s.as_str()), b.line, &[TargetKind::Cell, TargetKind::Range], "{sparklines}");
            need(v, tgt.is_some(), "requires a target range");
            need(v, meta.get("source").is_some(), "requires source:");
            if let Some(ty) = meta.get("type").and_then(|t| t.as_str()) {
                need(v, ["line", "column", "win-loss"].contains(&ty), "type must be line | column | win-loss");
            }
        }
        "pivot" => {
            need(v, pos.first().is_some(), "requires a name");
            let anchor_cell = b.args.anchor.as_deref().map(strip_sheet_qualifier).and_then(|a| parse_cell(&a));
            need(v, anchor_cell.is_some(), "requires `at <cell>`");
            need(v, meta.get("source").is_some(), "requires source:");
            if let Some(name) = pos.first() {
                let key = name.to_lowercase();
                if v.global_names.contains_key(&key) {
                    v.err(b.line, format!("pivot name collides with an existing name: {name}"));
                }
                v.global_names.insert(key, "pivot");
            }
        }
        "slicer" => {
            need(v, b.args.anchor.is_some(), "requires an anchor");
            need(v, meta.get("for").is_some() && meta.get("field").is_some(), "requires for: and field:");
        }
        "image" => {
            need(v, b.args.anchor.is_some(), "requires an anchor");
            let src = meta.get("src").and_then(|s| s.as_str());
            need(v, src.is_some(), "requires src:");
            if let Some(src) = src {
                if !is_safe_image_src(src) {
                    v.err(b.line, format!("image src fails the scheme allowlist: {src}"));
                }
            }
        }
        "shape" => {
            if let Some(kind) = pos.first() {
                if !SHAPE_KINDS.contains(&kind.as_str()) {
                    v.warn(b.line, format!("unknown shape kind {kind} — carry exotic geometry via fallback:"));
                }
            }
            need(v, b.args.anchor.is_some(), "requires an anchor");
        }
        "textbox" => {
            need(v, b.args.anchor.is_some(), "requires an anchor");
        }
        "checkbox" => {
            need(v, b.args.anchor.is_some(), "requires an anchor");
            let ok = match meta.get("linked") {
                None => true,
                Some(l) => parse_cell(&l.to_js_string().replace('$', "")).is_some(),
            };
            need(v, ok, "linked: must be a cell");
        }
        "comments" => {
            let tgt = target_of(v, &sheet_name, pos.first().map(|s| s.as_str()), b.line, &[TargetKind::Cell], "{comments}");
            need(v, tgt.is_some(), "requires a cell target");
            let list = meta.as_array();
            need(v, list.is_some(), "body must be a YAML list of comments");
            if let Some(list) = list {
                for c in list {
                    if c.get("by").is_none() || c.get("at").is_none() || c.get("text").is_none() {
                        v.err(b.line, "each comment requires by:, at:, text:");
                    }
                }
            }
        }
        "outline" => {
            if let Some(rows) = meta.get("rows").and_then(|r| r.as_array()) {
                for r in rows {
                    let range = r.get("range").map(|x| x.to_js_string()).unwrap_or_default();
                    if !is_num_range(&range) {
                        v.err(b.line, format!("outline rows range must be \"n:m\": {range}"));
                    }
                }
            }
            if let Some(cols) = meta.get("cols").and_then(|c| c.as_array()) {
                for c in cols {
                    let range = c.get("range").map(|x| x.to_js_string()).unwrap_or_default();
                    if !is_col_letter_range(&range) {
                        v.err(b.line, format!("outline cols range must be \"A:B\": {range}"));
                    }
                }
            }
        }
        "page" => {
            if let Some(o) = meta.get("orientation").and_then(|x| x.as_str()) {
                need(v, ["portrait", "landscape"].contains(&o), "orientation must be portrait | landscape");
            }
            need(v, !(meta.get("scale").is_some() && meta.get("fit").is_some()), "scale: and fit: are mutually exclusive");
        }
        "query" => {
            need(v, pos.first().is_some(), "requires a name");
            need(v, meta.get("source").is_some(), "requires source:");
            let steps_ok = match meta.get("steps") {
                None => true,
                Some(s) => s.is_array(),
            };
            need(v, steps_ok, "steps: must be a list");
        }
        "script" => {
            need(v, pos.first().is_some(), "requires a name");
            need(v, b.args.flags.get("lang").is_some(), "requires lang=");
            need(v, b.code.as_deref().map(|c| !c.trim().is_empty()).unwrap_or(false), "requires a code payload after ---");
        }
        "scenario" => {
            need(v, pos.first().is_some(), "requires a name");
            let cells_ok = matches!(meta.get("cells"), Some(Yaml::Hash(_)));
            need(v, cells_ok, "requires cells:");
            if let Some(Yaml::Hash(cells)) = meta.get("cells") {
                for (k, _) in cells {
                    if parse_cell(&k.key_str().replace('$', "")).is_none() {
                        v.err(b.line, format!("scenario cells key must be a cell: {}", k.key_str()));
                    }
                }
            }
        }
        "raw" => {
            let fmt = pos.first().map(|s| s.as_str());
            need(v, matches!(fmt, Some("ooxml") | Some("json") | Some("text")), "format must be ooxml | json | text");
            if let Some(part) = b.args.flags.get("part") {
                need(v, is_valid_part_path(part), &format!("part= fails package-path canonicalization: {part}"));
            }
            if let Some(enc) = b.args.flags.get("encoding") {
                need(v, enc == "base64", "encoding must be base64");
            }
        }
        _ => {}
    }
}

fn strip_sheet_qualifier(s: &str) -> String {
    match s.rfind('!') {
        Some(i) => s[i + 1..].to_string(),
        None => s.to_string(),
    }
}

fn is_col_letters(k: &str) -> bool {
    !k.is_empty() && k.len() <= 3 && k.bytes().all(|c| c.is_ascii_uppercase())
}

fn is_num_range(s: &str) -> bool {
    // ^\d+:\d+$
    match s.split_once(':') {
        Some((a, b)) => {
            !a.is_empty()
                && !b.is_empty()
                && a.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn is_col_letter_range(s: &str) -> bool {
    // ^[A-Z]{1,3}:[A-Z]{1,3}$
    match s.split_once(':') {
        Some((a, b)) => is_col_letters(a) && is_col_letters(b),
        None => false,
    }
}

fn validate_table(
    v: &mut Validator,
    b: &FenceBlock,
    sheet: Option<&mut SheetState>,
) {
    let pos = &b.args.positional;
    let meta = &b.meta;
    let name = pos.first();
    let name_ok = name
        .map(|n| is_table_name(n) && !is_cellish(n))
        .unwrap_or(false);
    if !name_ok {
        v.err(b.line, format!("{{{}}} {}", b.kind, "requires a valid table name"));
    }
    let anchor = b.args.anchor.as_deref().and_then(parse_cell);
    if anchor.is_none() {
        v.err(b.line, format!("{{{}}} {}", b.kind, "requires `at <cell>`"));
    }
    if let Some(name) = name {
        let key = name.to_lowercase();
        if v.global_names.contains_key(&key) {
            v.err(b.line, format!("table name collides with an existing name: {name}"));
        }
        v.global_names.insert(key, "table");
    }
    let anchor = match anchor {
        Some(a) => a,
        None => return,
    };
    if b.rows.is_empty() {
        v.err(b.line, format!("{{{}}} {}", b.kind, "requires payload rows"));
        return;
    }
    let header = meta.get("header").and_then(|h| h.as_bool()) != Some(false);
    let mut columns: Vec<String> = Vec::new();
    if let Some(st) = sheet {
        for (ri, row) in b.rows.iter().enumerate() {
            for (ci, cell_text) in row.cells.iter().enumerate() {
                let s = parse_scalar(cell_text);
                if let Scalar::Text { problem: Some(p), .. } = &s {
                    v.err(row.line, format!("table cell: {p}"));
                }
                if header && ri == 0 {
                    match &s {
                        Scalar::Text { value, .. } if !value.is_empty() => columns.push(value.clone()),
                        _ => v.err(row.line, format!("table header cells must be non-empty text (column {})", ci + 1)),
                    }
                    add_def(v, st, anchor.col + ci as i64, anchor.row + ri as i64, row.line, "{table} header");
                    continue;
                }
                if !matches!(s, Scalar::Blank) {
                    add_def(v, st, anchor.col + ci as i64, anchor.row + ri as i64, row.line, "{table}");
                }
            }
        }
        let lower: Vec<String> = columns.iter().map(|c| c.to_lowercase()).collect();
        for (i, c) in lower.iter().enumerate() {
            if lower.iter().position(|x| x == c) != Some(i) {
                v.err(b.line, format!("duplicate table column name: {}", columns[i]));
            }
        }
        let col_set: std::collections::HashSet<&str> = lower.iter().map(|s| s.as_str()).collect();
        let check_cols = |v: &mut Validator, obj: Option<&Yaml>, what: &str| {
            if let Some(Yaml::Hash(pairs)) = obj {
                for (k, _) in pairs {
                    let key = k.key_str();
                    if !col_set.contains(key.to_lowercase().as_str()) {
                        v.err(b.line, format!("{what} references unknown column: {key}"));
                    }
                }
            }
        };
        check_cols(v, meta.get("cols"), "cols");
        check_cols(v, meta.get("total"), "total");
        check_cols(v, meta.get("filter"), "filter");
        if let Some(sorts) = meta.get("sort").and_then(|s| s.as_array()) {
            for s in sorts {
                let col = s.get("col").map(|x| x.to_js_string()).unwrap_or_default();
                if !col_set.contains(col.to_lowercase().as_str()) {
                    v.err(b.line, format!("sort references unknown column: {col}"));
                }
            }
        }
        if let Some(Yaml::Hash(total)) = meta.get("total") {
            let total_row = anchor.row + b.rows.len() as i64;
            for (col_name, _) in total {
                let cn = col_name.key_str().to_lowercase();
                if let Some(ci) = lower.iter().position(|c| *c == cn) {
                    add_def(v, st, anchor.col + ci as i64, total_row, b.line, "{table} total");
                }
            }
        }
    }
}

fn is_table_name(s: &str) -> bool {
    // ^[A-Za-z_\\][A-Za-z0-9_.\\]{0,254}$
    let ok_first = |c: char| c.is_ascii_alphabetic() || c == '_' || c == '\\';
    let ok_rest = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '\\';
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if ok_first(c) => {}
        _ => return false,
    }
    let count = s.chars().count();
    count <= 255 && chars.all(ok_rest)
}

fn is_cellish(s: &str) -> bool {
    // ^[A-Za-z]{1,3}\d+$
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == 0 || i > 3 || i >= b.len() {
        return false;
    }
    b[i..].iter().all(|c| c.is_ascii_digit())
}
