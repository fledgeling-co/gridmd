//! A1-reference parsing (SPEC.md §8.2, Appendix A). Port of `js/src/refs.js`.

pub const MAX_COL: i64 = 16384; // XFD
pub const MAX_ROW: i64 = 1_048_576;

/// Column letters (`A`, `AB`, `XFD`) → 1-based column number.
pub fn col_to_num(letters: &str) -> i64 {
    let mut n: i64 = 0;
    for ch in letters.chars() {
        n = n * 26 + (ch as i64 - 64);
    }
    n
}

/// 1-based column number → letters.
pub fn num_to_col(mut n: i64) -> String {
    let mut s = String::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, char::from_u32(65 + r as u32).unwrap_or('A'));
        n = (n - 1 - r) / 26;
    }
    s
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub col: i64,
    pub row: i64,
}

/// Kinds of `@`/anchor targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    Cell,
    Range,
    Cols,
    Rows,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub kind: TargetKind,
    pub sheet: Option<String>,
    pub c1: i64,
    pub r1: i64,
    pub c2: i64,
    pub r2: i64,
}

fn is_ascii_upper_run(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_uppercase())
}

/// Parse a single cell reference (`$?COL$?ROW`). Returns `None` if malformed or
/// out of bounds. Mirrors `CELL_RE` in refs.js.
pub fn parse_cell(text: &str) -> Option<Cell> {
    let bytes = text.as_bytes();
    let mut i = 0;
    if i < bytes.len() && bytes[i] == b'$' {
        i += 1;
    }
    let col_start = i;
    while i < bytes.len() && bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    let col_str = &text[col_start..i];
    if !is_ascii_upper_run(col_str, 3) {
        return None;
    }
    if i < bytes.len() && bytes[i] == b'$' {
        i += 1;
    }
    let row_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let row_str = &text[row_start..i];
    // ^[1-9]\d{0,6}$ and must consume the whole string.
    if i != bytes.len() || row_str.is_empty() || row_str.len() > 7 || row_str.starts_with('0') {
        return None;
    }
    let col = col_to_num(col_str);
    let row: i64 = row_str.parse().ok()?;
    if col > MAX_COL || row > MAX_ROW {
        return None;
    }
    Some(Cell { col, row })
}

fn parse_col_range(text: &str) -> Option<(i64, i64)> {
    let (a, b) = text.split_once(':')?;
    let a = a.strip_prefix('$').unwrap_or(a);
    let b = b.strip_prefix('$').unwrap_or(b);
    if !is_ascii_upper_run(a, 3) || !is_ascii_upper_run(b, 3) {
        return None;
    }
    Some((col_to_num(a), col_to_num(b)))
}

fn parse_row_range(text: &str) -> Option<(i64, i64)> {
    let (a, b) = text.split_once(':')?;
    let a = a.strip_prefix('$').unwrap_or(a);
    let b = b.strip_prefix('$').unwrap_or(b);
    let valid = |s: &str| !s.is_empty() && s.len() <= 7 && !s.starts_with('0') && s.bytes().all(|c| c.is_ascii_digit());
    if !valid(a) || !valid(b) {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Parse a target: `cell | cell:cell | col:col | row:row` with an optional
/// leading `Sheet!` qualifier. Mirrors `parseTarget` in refs.js.
pub fn parse_target(input: &str) -> Option<Target> {
    let mut text = input;
    let mut sheet: Option<String> = None;
    if let Some(bang) = text.rfind('!') {
        let mut name = &text[..bang];
        let owned;
        if name.starts_with('\'') && name.ends_with('\'') && name.len() >= 2 {
            owned = name[1..name.len() - 1].replace("''", "'");
            name = &owned;
            sheet = Some(name.to_string());
        } else {
            sheet = Some(name.to_string());
        }
        text = &text[bang + 1..];
    }
    if let Some(cell) = parse_cell(text) {
        return Some(Target {
            kind: TargetKind::Cell,
            sheet,
            c1: cell.col,
            r1: cell.row,
            c2: cell.col,
            r2: cell.row,
        });
    }
    if text.contains(':') {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() == 2 {
            if let (Some(a), Some(b)) = (parse_cell(parts[0]), parse_cell(parts[1])) {
                return Some(Target {
                    kind: TargetKind::Range,
                    sheet,
                    c1: a.col.min(b.col),
                    r1: a.row.min(b.row),
                    c2: a.col.max(b.col),
                    r2: a.row.max(b.row),
                });
            }
            if let Some((c1, c2)) = parse_col_range(text) {
                if c1 <= MAX_COL && c2 <= MAX_COL {
                    return Some(Target {
                        kind: TargetKind::Cols,
                        sheet,
                        c1: c1.min(c2),
                        r1: 0,
                        c2: c1.max(c2),
                        r2: 0,
                    });
                }
            }
            if let Some((r1, r2)) = parse_row_range(text) {
                if r1 <= MAX_ROW && r2 <= MAX_ROW {
                    return Some(Target {
                        kind: TargetKind::Rows,
                        sheet,
                        c1: 0,
                        r1: r1.min(r2),
                        c2: 0,
                        r2: r1.max(r2),
                    });
                }
            }
        }
    }
    None
}

pub fn ref_key(col: i64, row: i64) -> (i64, i64) {
    (col, row)
}
