//! Cell scalar micro-grammar (SPEC.md §6). Port of `js/src/scalar.js`.

/// Closed set of Excel error values (SPEC §6 rule 9).
pub const ERROR_VALUES: [&str; 12] = [
    "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A",
    "#GETTING_DATA", "#SPILL!", "#CALC!", "#FIELD!", "#BLOCKED!",
];

#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Blank,
    Number(f64),
    Boolean(bool),
    /// ISO date (`YYYY-MM-DD[...]`).
    Date(String),
    /// ISO time (`hh:mm[:ss]`).
    Time(String),
    Error(String),
    Text {
        value: String,
        /// A parse problem to surface (unterminated quote / CSE).
        problem: Option<String>,
    },
    Formula {
        cse: bool,
        formula: String,
        cached: Option<Box<CachedScalar>>,
    },
}

/// The cached side of a `formula :: cached` split.
#[derive(Debug, Clone, PartialEq)]
pub enum CachedScalar {
    Value(Scalar),
    /// A cached value that was itself a formula — invalid.
    Invalid(String),
}

fn is_number(raw: &str) -> bool {
    // ^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?$
    let b = raw.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    // integer part
    if i >= b.len() {
        return false;
    }
    if b[i] == b'0' {
        i += 1;
    } else if b[i].is_ascii_digit() {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }
    // fraction
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    // exponent
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == b.len()
}

fn is_date(raw: &str) -> bool {
    // ^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?)?$
    let b = raw.as_bytes();
    let digit = |c: u8| c.is_ascii_digit();
    if b.len() < 10 {
        return false;
    }
    if !(digit(b[0]) && digit(b[1]) && digit(b[2]) && digit(b[3]) && b[4] == b'-'
        && digit(b[5]) && digit(b[6]) && b[7] == b'-' && digit(b[8]) && digit(b[9]))
    {
        return false;
    }
    if b.len() == 10 {
        return true;
    }
    // optional T time
    if b[10] != b'T' {
        return false;
    }
    is_time(&raw[11..])
}

fn is_time(raw: &str) -> bool {
    // ^\d{2}:\d{2}(:\d{2})?$
    let b = raw.as_bytes();
    let digit = |c: u8| c.is_ascii_digit();
    if b.len() != 5 && b.len() != 8 {
        return false;
    }
    if !(digit(b[0]) && digit(b[1]) && b[2] == b':' && digit(b[3]) && digit(b[4])) {
        return false;
    }
    if b.len() == 5 {
        return true;
    }
    b[5] == b':' && digit(b[6]) && digit(b[7])
}

/// `DATE_RE.test` — used by the model's YAML-scalar coercion (`model.js`).
pub fn is_date_str(s: &str) -> bool {
    is_date(s)
}

/// `TIME_RE.test` — used by the model's YAML-scalar coercion (`model.js`).
pub fn is_time_str(s: &str) -> bool {
    is_time(s)
}

/// Splits `formula :: cached` at the LAST ` :: ` outside double-quoted string
/// literals (SPEC §6). Returns `(head, Some(cached))` or `(text, None)`.
pub fn split_cached(text: &str) -> (String, Option<String>) {
    let bytes = text.as_bytes();
    let mut in_q = false;
    let mut idx: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'"' {
            in_q = !in_q;
            i += 1;
            continue;
        }
        if !in_q && ch == b' ' && text[i..].starts_with(" :: ") {
            idx = Some(i);
        }
        i += 1;
    }
    match idx {
        None => (text.to_string(), None),
        Some(i) => (text[..i].to_string(), Some(text[i + 4..].trim().to_string())),
    }
}

fn dq_unquote(raw: &str) -> Option<String> {
    // ^"((?:[^"]|"")*)"$
    let b = raw.as_bytes();
    if b.len() < 2 || b[0] != b'"' || b[b.len() - 1] != b'"' {
        return None;
    }
    let inner = &raw[1..raw.len() - 1];
    // Every bare `"` must be part of a doubled `""` pair.
    let ib = inner.as_bytes();
    let mut i = 0;
    while i < ib.len() {
        if ib[i] == b'"' {
            if i + 1 < ib.len() && ib[i + 1] == b'"' {
                i += 2;
                continue;
            }
            return None;
        }
        i += 1;
    }
    Some(inner.replace("\"\"", "\""))
}

/// Parse one cell scalar. `raw` must already be trimmed.
pub fn parse_scalar(raw: &str) -> Scalar {
    if raw.is_empty() {
        return Scalar::Blank;
    }
    if raw.starts_with("{=") {
        let (head, cached) = split_cached(raw);
        if !head.ends_with('}') {
            return Scalar::Text {
                value: raw.to_string(),
                problem: Some("unterminated CSE array formula".to_string()),
            };
        }
        return Scalar::Formula {
            cse: true,
            formula: head[2..head.len() - 1].to_string(),
            cached: parse_cached(cached),
        };
    }
    if raw.starts_with('=') {
        let (head, cached) = split_cached(raw);
        return Scalar::Formula {
            cse: false,
            formula: head[1..].to_string(),
            cached: parse_cached(cached),
        };
    }
    if let Some(rest) = raw.strip_prefix('\'') {
        return Scalar::Text {
            value: rest.to_string(),
            problem: None,
        };
    }
    if raw.starts_with('"') {
        return match dq_unquote(raw) {
            Some(v) => Scalar::Text {
                value: v,
                problem: None,
            },
            None => Scalar::Text {
                value: raw.to_string(),
                problem: Some("unterminated quoted text".to_string()),
            },
        };
    }
    if is_number(raw) {
        return Scalar::Number(raw.parse::<f64>().unwrap_or(f64::NAN));
    }
    let up = raw.to_uppercase();
    if up == "TRUE" || up == "FALSE" {
        return Scalar::Boolean(up == "TRUE");
    }
    if is_date(raw) {
        return Scalar::Date(raw.to_string());
    }
    if is_time(raw) {
        return Scalar::Time(raw.to_string());
    }
    if ERROR_VALUES.contains(&up.as_str()) {
        return Scalar::Error(up);
    }
    Scalar::Text {
        value: raw.to_string(),
        problem: None,
    }
}

fn parse_cached(cached_text: Option<String>) -> Option<Box<CachedScalar>> {
    let text = cached_text?;
    let v = parse_scalar(&text);
    if matches!(v, Scalar::Formula { .. }) {
        return Some(Box::new(CachedScalar::Invalid(
            "cached value must not be a formula".to_string(),
        )));
    }
    Some(Box::new(CachedScalar::Value(v)))
}
