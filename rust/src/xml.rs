//! Minimal non-validating XML parser for OOXML parts. Namespace prefixes are
//! stripped from element names; attributes keep their original keys with a
//! local-name lookup helper. Port of `js/src/xml.js`. Good for well-formed
//! machine-written XML only (used by the native `from-xlsx` fallback reader).

#[derive(Debug, Clone)]
pub struct XmlNode {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
    pub text: String,
}

impl XmlNode {
    fn new(name: String) -> Self {
        XmlNode {
            name,
            attrs: Vec::new(),
            children: Vec::new(),
            text: String::new(),
        }
    }

    /// Attribute by local name (`id` matches both `id` and `r:id`; exact wins).
    pub fn attr(&self, name: &str) -> Option<&str> {
        if let Some((_, v)) = self.attrs.iter().find(|(k, _)| k == name) {
            return Some(v);
        }
        self.attrs
            .iter()
            .find(|(k, _)| local(k) == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn one(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn all(&self, name: &str) -> Vec<&XmlNode> {
        self.children.iter().filter(|c| c.name == name).collect()
    }

    /// Deep text of an element (its text + descendants').
    pub fn text_of(&self) -> String {
        let mut out = self.text.clone();
        for c in &self.children {
            out.push_str(&c.text_of());
        }
        out
    }
}

fn local(name: &str) -> &str {
    match name.find(':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

pub fn decode_entities(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi) = s[i..].find(';') {
                let body = &s[i + 1..i + semi];
                if let Some(ch) = decode_entity_body(body) {
                    out.push(ch);
                    i += semi + 1;
                    continue;
                }
            }
        }
        // push one UTF-8 char
        let ch = s[i..].chars().next().unwrap_or('\u{fffd}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn decode_entity_body(body: &str) -> Option<char> {
    if let Some(rest) = body.strip_prefix('#') {
        let code = if let Some(hex) = rest.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            rest.parse::<u32>().ok()?
        };
        return char::from_u32(code);
    }
    match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

/// Parse XML into a tree. Returns the first top-level element (mirrors xml.js).
pub fn parse_xml(src: &str) -> XmlNode {
    let mut root = XmlNode::new("#root".to_string());
    // Stack of node paths (indices from root).
    let mut stack: Vec<XmlNode> = vec![root];
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let n = chars.len();
    while i < n {
        if chars[i] == '<' {
            // Special constructs.
            if starts_with(&chars, i, "<![CDATA[") {
                let end = find_seq(&chars, i + 9, "]]>");
                let content: String = chars[i + 9..end].iter().collect();
                stack.last_mut().unwrap().text.push_str(&content);
                i = end + 3;
                continue;
            }
            if starts_with(&chars, i, "<!--") {
                let end = find_seq(&chars, i + 4, "-->");
                i = end + 3;
                continue;
            }
            if starts_with(&chars, i, "<?") {
                let end = find_seq(&chars, i + 2, "?>");
                i = end + 2;
                continue;
            }
            if starts_with(&chars, i, "<!") {
                let end = find_char(&chars, i, '>');
                i = end + 1;
                continue;
            }
            if starts_with(&chars, i, "</") {
                let end = find_char(&chars, i, '>');
                if stack.len() > 1 {
                    let node = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(node);
                }
                i = end + 1;
                continue;
            }
            // Open tag.
            let end = find_char(&chars, i, '>');
            let inner: String = chars[i + 1..end].iter().collect();
            let self_close = inner.ends_with('/');
            let inner = inner.strip_suffix('/').unwrap_or(&inner);
            let (name, attrs) = parse_tag(inner);
            let mut el = XmlNode::new(local(&name).to_string());
            el.attrs = attrs;
            if self_close {
                stack.last_mut().unwrap().children.push(el);
            } else {
                stack.push(el);
            }
            i = end + 1;
        } else {
            let start = i;
            while i < n && chars[i] != '<' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let decoded = decode_entities(&text);
            stack.last_mut().unwrap().text.push_str(&decoded);
        }
    }
    // Unwind any unclosed nodes.
    while stack.len() > 1 {
        let node = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(node);
    }
    root = stack.pop().unwrap();
    root.children.into_iter().next().unwrap_or_else(|| XmlNode::new("#root".to_string()))
}

fn parse_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let inner = inner.trim();
    // name is up to first whitespace
    let (name, rest) = match inner.find(|c: char| c.is_whitespace()) {
        Some(p) => (&inner[..p], &inner[p..]),
        None => (inner, ""),
    };
    let mut attrs = Vec::new();
    let bytes: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != '=' && !bytes[i].is_whitespace() {
            i += 1;
        }
        let key: String = bytes[key_start..i].iter().collect();
        while i < bytes.len() && bytes[i].is_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == '=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == '"' || bytes[i] == '\'') {
                let q = bytes[i];
                i += 1;
                let val_start = i;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                let val: String = bytes[val_start..i].iter().collect();
                i += 1; // closing quote
                if !key.is_empty() {
                    attrs.push((key, decode_entities(&val)));
                }
            }
        } else if !key.is_empty() {
            // valueless attribute — ignore
        }
    }
    (name.to_string(), attrs)
}

fn starts_with(chars: &[char], i: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    if i + p.len() > chars.len() {
        return false;
    }
    chars[i..i + p.len()] == p[..]
}

fn find_seq(chars: &[char], from: usize, pat: &str) -> usize {
    let p: Vec<char> = pat.chars().collect();
    let mut i = from;
    while i + p.len() <= chars.len() {
        if chars[i..i + p.len()] == p[..] {
            return i;
        }
        i += 1;
    }
    chars.len()
}

fn find_char(chars: &[char], from: usize, c: char) -> usize {
    let mut i = from;
    while i < chars.len() && chars[i] != c {
        i += 1;
    }
    i
}
