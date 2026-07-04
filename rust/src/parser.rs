//! GridMD document parser (SPEC §2–§10, Appendix A). Port of `js/src/parser.js`.
//! Produces a block tree; semantic checks live in `validate.rs`.

use crate::diag::Diag;
use crate::yaml::{parse_yaml, try_props, Yaml};
use std::collections::HashMap;

pub fn is_reserved_kind(kind: &str) -> bool {
    matches!(
        kind,
        "sheet" | "grid" | "spill-cache" | "table" | "cf" | "validation" | "filter"
            | "chart" | "sparklines" | "pivot" | "slicer" | "image" | "shape" | "textbox"
            | "checkbox" | "comments" | "outline" | "page" | "query" | "script" | "scenario"
            | "raw"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Strict,
    Lenient,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub cells: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InfoArgs {
    pub positional: Vec<String>,
    pub flags: HashMap<String, String>,
    pub anchor: Option<String>,
    pub size: Option<(i64, i64)>,
}

#[derive(Debug, Clone)]
pub struct FenceBlock {
    pub kind: String,
    pub args: InfoArgs,
    pub body: Vec<String>,
    pub line: usize,
    /// Parsed YAML meta (a mapping or a list); `Null` for grid/spill-cache/raw.
    pub meta: Yaml,
    pub rows: Vec<Row>,
    pub code: Option<String>,
    pub payload: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AtBlock {
    pub target_text: String,
    pub line: usize,
    pub scalar_text: Option<String>,
    pub props: Option<Yaml>,
    pub body: Option<Yaml>,
}

#[derive(Debug, Clone)]
pub enum Block {
    At(AtBlock),
    Fence(FenceBlock),
}

impl Block {
    pub fn line(&self) -> usize {
        match self {
            Block::At(a) => a.line,
            Block::Fence(f) => f.line,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub line: usize,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub defs: usize,
    pub blocks: usize,
}

pub struct Document {
    pub frontmatter: Yaml,
    pub workbook_blocks: Vec<Block>,
    pub sheets: Vec<Sheet>,
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
    pub mode: Mode,
    pub stats: Stats,
}

fn ident_kind_char0(c: u8) -> bool {
    c.is_ascii_alphabetic()
}
fn ident_kind_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

/// Match a fence-open line: 3+ backticks, `{kind}`, then the rest.
/// Returns `(tick_count, kind, rest)`.
fn match_fence_open(line: &str) -> Option<(usize, String, String)> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] == b'`' {
        i += 1;
    }
    if i < 3 || i >= b.len() || b[i] != b'{' {
        return None;
    }
    let ticks = i;
    i += 1; // consume '{'
    let kind_start = i;
    if i >= b.len() || !ident_kind_char0(b[i]) {
        return None;
    }
    i += 1;
    while i < b.len() && ident_kind_char(b[i]) {
        i += 1;
    }
    let kind = line[kind_start..i].to_string();
    if i >= b.len() || b[i] != b'}' {
        return None;
    }
    i += 1; // consume '}'
    let rest = line[i..].to_string();
    Some((ticks, kind, rest))
}

/// A fence-close line: `>= open` backticks then only whitespace.
fn is_fence_close(line: &str, open: usize) -> bool {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] == b'`' {
        i += 1;
    }
    if i < open {
        return false;
    }
    line[i..].chars().all(|c| c.is_whitespace())
}

/// Info-string args: positional (quoted `""` doubling), `at` anchors, `size WxH`,
/// `key=val` flags. Port of `parseInfoArgs`.
pub fn parse_info_args(rest: &str, line: usize, errors: &mut Vec<Diag>) -> InfoArgs {
    let mut out = InfoArgs::default();
    let tokens = tokenize_info(rest);
    let mut k = 0;
    while k < tokens.len() {
        let tok = &tokens[k];
        if !tok.quoted && tok.value == "at" {
            k += 1;
            if k >= tokens.len() {
                errors.push(Diag::new(line, "`at` requires an anchor"));
                break;
            }
            out.anchor = Some(tokens[k].value.clone());
            k += 1;
            continue;
        }
        if !tok.quoted && tok.value == "size" {
            k += 1;
            let sm = tokens.get(k).and_then(|t| parse_size(&t.value));
            match sm {
                Some((w, h)) => out.size = Some((w, h)),
                None => errors.push(Diag::new(line, "`size` requires WxH (e.g. 480x320)")),
            }
            k += 1;
            continue;
        }
        if !tok.quoted {
            if let Some((key, val)) = parse_flag(&tok.value) {
                out.flags.insert(key, val);
                k += 1;
                continue;
            }
        }
        out.positional.push(tok.value.clone());
        k += 1;
    }
    out
}

struct InfoToken {
    value: String,
    quoted: bool,
}

fn tokenize_info(rest: &str) -> Vec<InfoToken> {
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    let mut tokens = Vec::new();
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '"' {
            // "((?:[^"]|"")*)"
            i += 1;
            let mut val = String::new();
            while i < chars.len() {
                if chars[i] == '"' {
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        val.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                val.push(chars[i]);
                i += 1;
            }
            tokens.push(InfoToken {
                value: val,
                quoted: true,
            });
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            tokens.push(InfoToken {
                value: chars[start..i].iter().collect(),
                quoted: false,
            });
        }
    }
    tokens
}

fn parse_size(s: &str) -> Option<(i64, i64)> {
    let (a, b) = s.split_once('x')?;
    if a.is_empty() || b.is_empty() || !a.bytes().all(|c| c.is_ascii_digit())
        || !b.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn parse_flag(tok: &str) -> Option<(String, String)> {
    // ^([A-Za-z][A-Za-z0-9-]*)=(.*)$
    let eq = tok.find('=')?;
    let key = &tok[..eq];
    let kb = key.as_bytes();
    if kb.is_empty() || !kb[0].is_ascii_alphabetic() {
        return None;
    }
    if !kb.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'-') {
        return None;
    }
    let mut val = tok[eq + 1..].to_string();
    // .replace(/^"(.*)"$/s, '$1').replace(/""/g, '"')
    if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
        val = val[1..val.len() - 1].to_string();
    }
    val = val.replace("\"\"", "\"");
    Some((key.to_string(), val))
}

/// Right-edge props split (SPEC §9.1 / Appendix A). Returns `(scalar_text, props_text)`.
pub fn find_props_split(text: &str) -> (String, Option<String>) {
    if !text.ends_with('}') {
        return (text.to_string(), None);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut in_q = false;
    let mut depth: i32 = 0;
    let mut start: i64 = -1;
    let mut last_group: Option<(usize, usize)> = None;
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '"' {
            in_q = !in_q;
            continue;
        }
        if in_q {
            continue;
        }
        if ch == '{' {
            if depth == 0 {
                start = i as i64;
            }
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 && start != -1 {
                last_group = Some((start as usize, i));
            }
            if depth < 0 {
                return (text.to_string(), None);
            }
        }
    }
    let (s, e) = match last_group {
        Some(g) => g,
        None => return (text.to_string(), None),
    };
    if e != chars.len() - 1 || s == 0 || chars[s - 1] != ' ' {
        return (text.to_string(), None);
    }
    let scalar: String = chars[..s].iter().collect::<String>().trim_end().to_string();
    let props: String = chars[s..].iter().collect();
    (scalar, Some(props))
}

/// Pipe row → trimmed cell strings; backslash escapes the next character.
/// Returns `None` if the line is not a well-formed pipe row.
pub fn split_pipe_row(raw_line: &str) -> Option<Vec<String>> {
    let line = raw_line.trim_end();
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() || chars[0] != '|' || chars.len() < 2 {
        return None;
    }
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut opened = false;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            cell.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if ch == '|' {
            if !opened {
                opened = true;
                i += 1;
                continue;
            }
            cells.push(cell.trim().to_string());
            cell.clear();
            i += 1;
            continue;
        }
        cell.push(ch);
        i += 1;
    }
    if !cell.trim().is_empty() {
        return None; // no unescaped closing pipe
    }
    Some(cells)
}

fn parse_rows(body_lines: &[String], base_line: usize, errors: &mut Vec<Diag>) -> Vec<Row> {
    let mut rows = Vec::new();
    for (k, l) in body_lines.iter().enumerate() {
        if l.trim().is_empty() {
            continue;
        }
        match split_pipe_row(l) {
            None => {
                let snippet: String = l.chars().take(50).collect();
                errors.push(Diag::new(
                    base_line + k + 1,
                    format!("expected a pipe row, got: {snippet}"),
                ));
            }
            Some(cells) => rows.push(Row {
                cells,
                line: base_line + k + 1,
            }),
        }
    }
    rows
}

fn refine_fence(block: &mut FenceBlock, errors: &mut Vec<Diag>) {
    let line = block.line;
    let kind = block.kind.clone();
    let body = block.body.clone();
    let meta_of = |arr: &[String], off: usize, errors: &mut Vec<Diag>| -> Yaml {
        parse_yaml(&arr.join("\n"), line + off, errors)
    };
    if kind == "grid" || kind == "spill-cache" {
        block.rows = parse_rows(&body, line, errors);
    } else if kind == "table" {
        let d = body.iter().position(|l| l == "---");
        match d {
            None => {
                errors.push(Diag::new(
                    line,
                    "{table} requires a `---`-separated payload of pipe rows",
                ));
                block.meta = meta_of(&body, 1, errors);
                block.rows = Vec::new();
            }
            Some(d) => {
                block.meta = meta_of(&body[..d], 1, errors);
                block.rows = parse_rows(&body[d + 1..], line + d + 1, errors);
            }
        }
    } else if kind == "script" {
        let d = body.iter().position(|l| l == "---");
        match d {
            None => {
                block.meta = Yaml::Hash(Vec::new());
                block.code = Some(body.join("\n"));
            }
            Some(d) => {
                block.meta = meta_of(&body[..d], 1, errors);
                block.code = Some(body[d + 1..].join("\n"));
            }
        }
    } else if kind == "raw" || kind.starts_with("x-") {
        block.payload = Some(body.join("\n"));
    } else {
        block.meta = meta_of(&body, 1, errors);
    }
}

fn parse_fence(
    lines: &[String],
    i: usize,
    ticks: usize,
    kind: String,
    rest: String,
    errors: &mut Vec<Diag>,
) -> (Block, usize) {
    let args = parse_info_args(&rest, i + 1, errors);
    let mut body = Vec::new();
    let mut j = i + 1;
    let mut closed = false;
    while j < lines.len() {
        if is_fence_close(&lines[j], ticks) {
            closed = true;
            j += 1;
            break;
        }
        body.push(lines[j].clone());
        j += 1;
    }
    if !closed {
        errors.push(Diag::new(i + 1, format!("unclosed {{{kind}}} fence")));
    }
    let mut block = FenceBlock {
        kind,
        args,
        body,
        line: i + 1,
        meta: Yaml::Hash(Vec::new()),
        rows: Vec::new(),
        code: None,
        payload: None,
    };
    refine_fence(&mut block, errors);
    (Block::Fence(block), j)
}

fn parse_at(lines: &[String], i: usize, errors: &mut Vec<Diag>) -> (Block, usize) {
    let line = &lines[i];
    let rest = &line[2..];
    let sp = rest.find(' ');
    let target_text = match sp {
        None => rest.to_string(),
        Some(sp) => rest[..sp].to_string(),
    };
    let inline = match sp {
        None => String::new(),
        Some(sp) => rest[sp + 1..].trim().to_string(),
    };

    // Multiline body: maximal run of blank-or-2-space-indented lines (dedent rule).
    let mut j = i + 1;
    let mut taken = 0usize;
    let mut last_take = 0usize;
    let mut acc: Vec<String> = Vec::new();
    while j < lines.len() {
        let l = &lines[j];
        if l.trim().is_empty() {
            acc.push(String::new());
            j += 1;
            taken += 1;
            continue;
        }
        if l.starts_with("  ") {
            acc.push(l[2..].to_string());
            j += 1;
            taken += 1;
            last_take = taken;
            continue;
        }
        break;
    }
    let body_lines: Option<Vec<String>> = if last_take > 0 {
        Some(acc[..last_take].to_vec())
    } else {
        None
    };
    let next = i + 1 + last_take;

    let mut block = AtBlock {
        target_text,
        line: i + 1,
        scalar_text: None,
        props: None,
        body: None,
    };
    if let Some(bl) = body_lines {
        let parsed = parse_yaml(&bl.join("\n"), i + 2, errors);
        match parsed {
            Yaml::Hash(_) => block.body = Some(parsed),
            _ => errors.push(Diag::new(i + 2, "@ directive body must be a YAML mapping")),
        }
    }
    if !inline.is_empty() {
        if inline.starts_with('{') && !inline.starts_with("{=") {
            if let Some(props) = try_props(&inline) {
                block.props = Some(props);
                return (Block::At(block), next);
            }
        }
        let (scalar_text, props_text) = find_props_split(&inline);
        if let Some(pt) = props_text {
            if let Some(props) = try_props(&pt) {
                block.props = Some(props);
                block.scalar_text = if scalar_text.is_empty() {
                    None
                } else {
                    Some(scalar_text)
                };
                return (Block::At(block), next);
            }
        }
        block.scalar_text = Some(inline);
    }
    (Block::At(block), next)
}

fn split_lines(source: &str) -> Vec<String> {
    source
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect()
}

fn is_heading2plus(line: &str) -> bool {
    // ^#{2,}(\s|$)
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] == b'#' {
        i += 1;
    }
    i >= 2 && (i == b.len() || b[i].is_ascii_whitespace())
}

fn heading1(line: &str) -> Option<String> {
    // ^# (.+)$
    let rest = line.strip_prefix("# ")?;
    if rest.is_empty() {
        None
    } else {
        Some(rest.trim().to_string())
    }
}

pub fn parse_document(source: &str, mode: Mode) -> Document {
    let lines = split_lines(source);
    let mut doc = Document {
        frontmatter: Yaml::Hash(Vec::new()),
        workbook_blocks: Vec::new(),
        sheets: Vec::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
        mode,
        stats: Stats::default(),
    };

    if lines.first().map(|s| s.as_str()) != Some("---") {
        doc.errors.push(Diag::new(
            1,
            "document must begin with `---` YAML frontmatter",
        ));
        return doc;
    }
    let mut fm_end: i64 = -1;
    for (k, l) in lines.iter().enumerate().skip(1) {
        if l == "---" {
            fm_end = k as i64;
            break;
        }
    }
    if fm_end == -1 {
        doc.errors.push(Diag::new(
            1,
            "unterminated frontmatter (missing closing `---`)",
        ));
        return doc;
    }
    let fm_end = fm_end as usize;
    doc.frontmatter = parse_yaml(&lines[1..fm_end].join("\n"), 2, &mut doc.errors);

    let mut i = fm_end + 1;
    // index into doc.sheets for the current sheet, or None (workbook scope)
    let mut cur: Option<usize> = None;

    while i < lines.len() {
        let line = &lines[i];
        if line.trim().is_empty() || line.starts_with('>') || is_heading2plus(line) {
            i += 1;
            continue;
        }
        if let Some(name) = heading1(line) {
            doc.sheets.push(Sheet {
                name,
                line: i + 1,
                blocks: Vec::new(),
            });
            cur = Some(doc.sheets.len() - 1);
            i += 1;
            continue;
        }
        if let Some((ticks, kind, rest)) = match_fence_open(line) {
            let (block, next) = parse_fence(&lines, i, ticks, kind, rest, &mut doc.errors);
            push_block(&mut doc, cur, block);
            i = next;
            continue;
        }
        if line.starts_with("@ ") {
            let (block, next) = parse_at(&lines, i, &mut doc.errors);
            push_block(&mut doc, cur, block);
            i = next;
            continue;
        }
        let snippet: String = line.chars().take(60).collect();
        let diag = Diag::new(i + 1, format!("unrecognized line: {snippet}"));
        match mode {
            Mode::Strict => doc.errors.push(diag),
            Mode::Lenient => doc.warnings.push(diag),
        }
        i += 1;
    }
    doc
}

fn push_block(doc: &mut Document, cur: Option<usize>, block: Block) {
    match cur {
        Some(idx) => doc.sheets[idx].blocks.push(block),
        None => doc.workbook_blocks.push(block),
    }
}
