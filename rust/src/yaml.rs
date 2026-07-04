//! A small YAML value model over the `saphyr-parser` event stream, restricted
//! to the GridMD safe subset (SPEC §3; `js/src/parser.js` `parseYaml`/`tryProps`).
//!
//! Only maps/lists/flow collections/block scalars/quoted+plain scalars are
//! supported. Anchors, aliases and explicit tags are detected and rejected the
//! way `validate.js`/`parseYaml` do. Scalar resolution follows the YAML 1.2
//! core schema for plain scalars; quoted and block scalars are always strings.

use crate::diag::Diag;
use crate::dump::format_number;
use saphyr_parser::{Event, Parser, ScalarStyle};

#[derive(Debug, Clone, PartialEq)]
pub enum Yaml {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Str(String),
    Array(Vec<Yaml>),
    /// Insertion-ordered mapping. Keys keep their resolved node so lookups can
    /// stringify them the way ECMAScript object keys do.
    Hash(Vec<(Yaml, Yaml)>),
}

/// Result of loading one YAML document from text.
pub struct Loaded {
    pub value: Yaml,
    pub has_anchor_or_alias: bool,
    pub has_tag: bool,
}

fn resolve_plain(s: &str) -> Yaml {
    match s {
        "" | "~" | "null" | "Null" | "NULL" => return Yaml::Null,
        "true" | "True" | "TRUE" => return Yaml::Bool(true),
        "false" | "False" | "FALSE" => return Yaml::Bool(false),
        _ => {}
    }
    if is_yaml_int(s) {
        if let Ok(n) = s.parse::<i64>() {
            return Yaml::Int(n);
        }
    }
    if is_yaml_float(s) {
        if let Ok(f) = s.parse::<f64>() {
            return Yaml::Real(f);
        }
    }
    Yaml::Str(s.to_string())
}

fn is_yaml_int(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit())
}

fn is_yaml_float(s: &str) -> bool {
    // [-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    } else {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
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

/// Build a `Yaml` tree from the event stream. Returns an error string on a
/// scanner/parser failure.
pub fn load(text: &str) -> Result<Loaded, String> {
    let mut events: Vec<Event> = Vec::new();
    for item in Parser::new_from_str(text) {
        match item {
            Ok((ev, _span)) => events.push(ev),
            Err(e) => return Err(first_line(&e.to_string())),
        }
    }
    let mut state = Builder {
        events: &events,
        pos: 0,
        has_anchor_or_alias: false,
        has_tag: false,
    };
    // Skip StreamStart / DocumentStart, build the first node, if any.
    state.skip_prelude();
    let value = if state.at_end() {
        Yaml::Null
    } else {
        state.build_node()
    };
    Ok(Loaded {
        value,
        has_anchor_or_alias: state.has_anchor_or_alias,
        has_tag: state.has_tag,
    })
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).to_string()
}

struct Builder<'a> {
    events: &'a [Event<'a>],
    pos: usize,
    has_anchor_or_alias: bool,
    has_tag: bool,
}

impl<'a> Builder<'a> {
    fn at_end(&self) -> bool {
        self.pos >= self.events.len()
            || matches!(
                self.events[self.pos],
                Event::StreamEnd | Event::DocumentEnd
            )
    }

    fn skip_prelude(&mut self) {
        while self.pos < self.events.len() {
            match &self.events[self.pos] {
                Event::StreamStart | Event::DocumentStart(_) | Event::Nothing => self.pos += 1,
                _ => break,
            }
        }
    }

    fn build_node(&mut self) -> Yaml {
        if self.pos >= self.events.len() {
            return Yaml::Null;
        }
        let ev = &self.events[self.pos];
        self.pos += 1;
        match ev {
            Event::Scalar(v, style, anchor, tag) => {
                if *anchor != 0 {
                    self.has_anchor_or_alias = true;
                }
                if tag.is_some() {
                    self.has_tag = true;
                }
                if *style == ScalarStyle::Plain {
                    resolve_plain(v)
                } else {
                    Yaml::Str(v.to_string())
                }
            }
            Event::Alias(_) => {
                self.has_anchor_or_alias = true;
                Yaml::Null
            }
            Event::SequenceStart(anchor, tag) => {
                if *anchor != 0 {
                    self.has_anchor_or_alias = true;
                }
                if tag.is_some() {
                    self.has_tag = true;
                }
                let mut items = Vec::new();
                while self.pos < self.events.len()
                    && !matches!(self.events[self.pos], Event::SequenceEnd)
                {
                    items.push(self.build_node());
                }
                if self.pos < self.events.len() {
                    self.pos += 1; // consume SequenceEnd
                }
                Yaml::Array(items)
            }
            Event::MappingStart(anchor, tag) => {
                if *anchor != 0 {
                    self.has_anchor_or_alias = true;
                }
                if tag.is_some() {
                    self.has_tag = true;
                }
                let mut pairs = Vec::new();
                while self.pos < self.events.len()
                    && !matches!(self.events[self.pos], Event::MappingEnd)
                {
                    let key = self.build_node();
                    let val = if self.pos < self.events.len()
                        && !matches!(self.events[self.pos], Event::MappingEnd)
                    {
                        self.build_node()
                    } else {
                        Yaml::Null
                    };
                    pairs.push((key, val));
                }
                if self.pos < self.events.len() {
                    self.pos += 1; // consume MappingEnd
                }
                Yaml::Hash(pairs)
            }
            _ => Yaml::Null,
        }
    }
}

impl Yaml {
    /// JS `Object.keys`-style stringification of a mapping key.
    pub fn key_str(&self) -> String {
        self.to_js_string()
    }

    /// ECMAScript `String(value)` for a scalar-ish value.
    pub fn to_js_string(&self) -> String {
        match self {
            Yaml::Null => "null".to_string(),
            Yaml::Bool(b) => b.to_string(),
            Yaml::Int(n) => n.to_string(),
            Yaml::Real(f) => format_number(*f),
            Yaml::Str(s) => s.clone(),
            Yaml::Array(_) => String::new(),
            Yaml::Hash(_) => "[object Object]".to_string(),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Yaml::Null)
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Yaml::Array(_))
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Yaml::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Yaml::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Yaml::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Yaml]> {
        match self {
            Yaml::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_hash(&self) -> Option<&[(Yaml, Yaml)]> {
        match self {
            Yaml::Hash(h) => Some(h),
            _ => None,
        }
    }

    /// Look up a mapping value by (stringified) key.
    pub fn get(&self, key: &str) -> Option<&Yaml> {
        match self {
            Yaml::Hash(pairs) => pairs
                .iter()
                .find(|(k, _)| k.key_str() == key)
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// `true` when a mapping has the key with a non-null value (JS `!== undefined`).
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

fn ident_key(k: &str) -> bool {
    // ^(x-)?[a-z][a-z0-9-]*$
    let body = k.strip_prefix("x-").unwrap_or(k);
    let mut chars = body.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Parse a YAML block/flow document for a directive body or frontmatter,
/// pushing diagnostics for scanner failures and safe-subset violations.
/// Returns an empty map for blank input (JS `parseYaml` returns `{}`).
pub fn parse_yaml(text: &str, line: usize, errors: &mut Vec<Diag>) -> Yaml {
    if text.trim().is_empty() {
        return Yaml::Hash(Vec::new());
    }
    match load(text) {
        Ok(loaded) => {
            if loaded.has_anchor_or_alias {
                errors.push(Diag::new(
                    line,
                    "YAML anchors/aliases are outside the GridMD safe subset",
                ));
            }
            if loaded.has_tag {
                errors.push(Diag::new(
                    line,
                    "YAML tags are outside the GridMD safe subset",
                ));
            }
            match loaded.value {
                Yaml::Null => Yaml::Hash(Vec::new()),
                other => other,
            }
        }
        Err(msg) => {
            errors.push(Diag::new(line, format!("YAML: {msg}")));
            Yaml::Hash(Vec::new())
        }
    }
}

/// YAML flow-map candidate for `@`-directive props (SPEC Appendix A, props rule):
/// must parse to a mapping whose top-level keys are all identifiers and whose
/// values are all non-null. Returns the mapping, or `None`.
pub fn try_props(text: &str) -> Option<Yaml> {
    let loaded = load(text).ok()?;
    let value = loaded.value;
    let pairs = match &value {
        Yaml::Hash(pairs) => pairs,
        _ => return None,
    };
    for (k, v) in pairs {
        if !ident_key(&k.key_str()) {
            return None;
        }
        if v.is_null() {
            return None;
        }
    }
    Some(value)
}
