// A focused YAML-subset parser for GridMD (SPEC.md §10 "safe subset": no
// anchors/aliases/tags; YAML-1.2-core-ish scalars). It reproduces the value
// tree the JS reference obtains from the `yaml` library's `.toJS()` for the
// constructs GridMD uses: block maps, block sequences, flow maps/sequences,
// literal/folded block scalars, single/double-quoted and plain scalars with
// core-schema type inference, and `#` comments.
//
// Map key order is preserved (as an ordered pair array) even though the dump
// never depends on it — it keeps the model faithful and the carry lossless.

import Foundation

indirect enum YamlValue: Equatable {
    case null
    case bool(Bool)
    case int(Int)
    case double(Double)
    case string(String)
    case list([YamlValue])
    case map([Pair])

    struct Pair: Equatable {
        let key: String
        let value: YamlValue
        init(_ key: String, _ value: YamlValue) { self.key = key; self.value = value }
    }

    // MARK: convenience accessors

    subscript(_ key: String) -> YamlValue? {
        if case let .map(entries) = self { return entries.first { $0.key == key }?.value }
        return nil
    }

    /// Presence test mirroring `x !== undefined` in the JS reference.
    func has(_ key: String) -> Bool { self[key] != nil }

    var isMap: Bool { if case .map = self { return true }; return false }
    var isList: Bool { if case .list = self { return true }; return false }

    var listItems: [YamlValue] { if case let .list(v) = self { return v }; return [] }
    var mapEntries: [Pair] { if case let .map(v) = self { return v }; return [] }

    var asString: String? { if case let .string(v) = self { return v }; return nil }
    var asBool: Bool? { if case let .bool(v) = self { return v }; return nil }
    var asInt: Int? { if case let .int(v) = self { return v }; return nil }
    var asDouble: Double? { if case let .double(v) = self { return v }; return nil }

    var count: Int {
        switch self {
        case let .list(v): return v.count
        case let .map(v): return v.count
        default: return 0
        }
    }

    /// JS truthiness (arrays and objects are always truthy, even when empty).
    var isTruthy: Bool {
        switch self {
        case .null: return false
        case let .bool(b): return b
        case let .int(i): return i != 0
        case let .double(d): return d != 0
        case let .string(s): return !s.isEmpty
        case .list, .map: return true
        }
    }

    /// JS `String(value)` for scalar values (used by the dump's `names[].value`).
    var jsString: String {
        switch self {
        case .null: return "null"
        case let .bool(b): return b ? "true" : "false"
        case let .int(i): return String(i)
        case let .double(d): return ESNumber.string(d)
        case let .string(s): return s
        case .list, .map: return "" // never reached for the fields we stringify
        }
    }
}

struct YamlParseResult {
    let value: YamlValue
    let errors: [String]
}

enum Yaml {
    /// Parses a whole YAML document (block context). Empty/whitespace/comment-only
    /// input yields an empty map `{}` (mirroring the JS `doc.toJS() ?? {}`).
    static func parse(_ text: String) -> YamlParseResult {
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return YamlParseResult(value: .map([]), errors: [])
        }
        var parser = BlockParser(lines: text.components(separatedBy: "\n"))
        let value = parser.parseDocument()
        return YamlParseResult(value: value, errors: parser.errors)
    }

    /// Candidate flow-map for `@`-directive props (SPEC Appendix A, props rule):
    /// must be a mapping whose every key is an identifier and every value non-null.
    static func tryProps(_ text: String) -> YamlValue? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        guard trimmed.hasPrefix("{") else { return nil }
        guard let value = FlowParser.parseFlow(trimmed), value.isMap else { return nil }
        for entry in value.mapEntries {
            if !isIdentKey(entry.key) { return nil }
            if case .null = entry.value { return nil }
        }
        return value
    }

    private static let identKeyRE = try! NSRegularExpression(pattern: "^(x-)?[a-z][a-z0-9-]*$")
    static func isIdentKey(_ k: String) -> Bool { matches(identKeyRE, k) }
}

// MARK: - Block parser

private struct BlockParser {
    var lines: [String]
    var pos = 0
    var errors: [String] = []

    mutating func parseDocument() -> YamlValue {
        skipBlankComment()
        if pos >= lines.count { return .map([]) }
        let node = parseNode(minIndent: indentOf(lines[pos]))
        if case .null = node { return .map([]) }
        return node
    }

    mutating func skipBlankComment() {
        while pos < lines.count {
            let t = lines[pos].trimmingCharacters(in: .whitespaces)
            if t.isEmpty || t.hasPrefix("#") { pos += 1 } else { break }
        }
    }

    func indentOf(_ line: String) -> Int {
        var n = 0
        for ch in line {
            if ch == " " { n += 1 } else { break }
        }
        return n
    }

    func contentAfter(_ line: String, _ indent: Int) -> String {
        String(line.dropFirst(indent))
    }

    mutating func parseNode(minIndent: Int) -> YamlValue {
        skipBlankComment()
        if pos >= lines.count { return .null }
        let ind = indentOf(lines[pos])
        if ind < minIndent { return .null }
        let content = contentAfter(lines[pos], ind)
        if content == "-" || content.hasPrefix("- ") {
            return parseSequence(indent: ind)
        }
        if mappingColon(content) != nil {
            return parseMapping(indent: ind)
        }
        // Single-line scalar/flow node.
        pos += 1
        return scalarFrom(stripComment(content))
    }

    mutating func parseSequence(indent seqIndent: Int) -> YamlValue {
        var items: [YamlValue] = []
        while true {
            skipBlankComment()
            if pos >= lines.count { break }
            let line = lines[pos]
            let ind = indentOf(line)
            if ind != seqIndent { break }
            let content = contentAfter(line, ind)
            if !(content == "-" || content.hasPrefix("- ")) { break }

            let chars = Array(line)
            var c = ind + 1
            while c < chars.count, chars[c] == " " { c += 1 }
            if c >= chars.count || chars[c] == "#" {
                pos += 1
                items.append(parseNode(minIndent: seqIndent + 1))
            } else {
                lines[pos] = String(repeating: " ", count: c) + String(chars[c...])
                items.append(parseNode(minIndent: c))
            }
        }
        return .list(items)
    }

    mutating func parseMapping(indent mapIndent: Int) -> YamlValue {
        var entries: [YamlValue.Pair] = []
        while true {
            skipBlankComment()
            if pos >= lines.count { break }
            let line = lines[pos]
            let ind = indentOf(line)
            if ind != mapIndent { break }
            let content = contentAfter(line, ind)
            guard let colonIdx = mappingColon(content) else { break }

            let keyText = String(content[content.startIndex..<colonIdx]).trimmingCharacters(in: .whitespaces)
            let key = unquoteKey(keyText)
            let afterColon = String(content[content.index(after: colonIdx)...])
            let inlineValue = stripComment(afterColon).trimmingCharacters(in: .whitespaces)
            pos += 1

            let value: YamlValue
            if inlineValue.isEmpty {
                value = parseNode(minIndent: mapIndent + 1)
            } else if isBlockScalarIndicator(inlineValue) {
                value = parseBlockScalar(indicator: inlineValue, parentIndent: mapIndent)
            } else {
                value = scalarFrom(inlineValue)
            }
            entries.append(YamlValue.Pair(key, value))
        }
        return .map(entries)
    }

    func isBlockScalarIndicator(_ s: String) -> Bool {
        guard let first = s.first, first == "|" || first == ">" else { return false }
        for ch in s.dropFirst() where !(ch == "-" || ch == "+" || ch.isNumber) { return false }
        return true
    }

    mutating func parseBlockScalar(indicator: String, parentIndent: Int) -> YamlValue {
        let literal = indicator.first == "|"
        var chomp: Character?
        var explicitIndent: Int?
        for ch in indicator.dropFirst() {
            if ch == "-" || ch == "+" { chomp = ch }
            else if let d = ch.wholeNumberValue { explicitIndent = d }
        }

        var raw: [String] = []
        while pos < lines.count {
            let line = lines[pos]
            if line.trimmingCharacters(in: .whitespaces).isEmpty {
                raw.append("")
                pos += 1
                continue
            }
            if indentOf(line) <= parentIndent { break }
            raw.append(line)
            pos += 1
        }
        // Drop trailing blank lines that belong to the document, remembering how
        // many there were (for '+' keep chomping).
        var trailing = 0
        while let last = raw.last, last.isEmpty {
            raw.removeLast()
            trailing += 1
        }
        if raw.isEmpty { return .string("") }

        let contentIndent: Int
        if let explicitIndent {
            contentIndent = parentIndent + explicitIndent
        } else {
            contentIndent = raw.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                .map { indentOf($0) }.min() ?? 0
        }
        let stripped = raw.map { line -> String in
            let count = min(contentIndent, indentOf(line))
            return String(line.dropFirst(count))
        }

        var text: String
        if literal {
            text = stripped.joined(separator: "\n")
        } else {
            text = foldLines(stripped)
        }
        switch chomp {
        case "-"?:
            break
        case "+"?:
            text += String(repeating: "\n", count: 1 + trailing)
        default:
            text += "\n"
        }
        return .string(text)
    }

    /// Folded (`>`) scalar: blank lines become newlines, runs of non-blank lines
    /// join with a single space.
    private func foldLines(_ lines: [String]) -> String {
        var out = ""
        var prevBlank = true
        for line in lines {
            if line.isEmpty {
                out += "\n"
                prevBlank = true
            } else {
                if !prevBlank { out += " " }
                out += line
                prevBlank = false
            }
        }
        return out
    }
}

// MARK: - Flow parser

enum FlowParser {
    /// Parses a flow scalar/collection: `{…}`, `[…]`, quoted, or plain.
    static func parseFlow(_ raw: String) -> YamlValue? {
        let s = raw.trimmingCharacters(in: .whitespaces)
        if s.isEmpty { return .null }
        if s.hasPrefix("{") { return parseFlowMap(s) }
        if s.hasPrefix("[") { return parseFlowSeq(s) }
        return scalarLeaf(s)
    }

    static func parseFlowMap(_ s: String) -> YamlValue? {
        guard s.hasPrefix("{"), s.hasSuffix("}") else { return nil }
        let inner = String(s.dropFirst().dropLast())
        var entries: [YamlValue.Pair] = []
        for part in splitTopLevel(inner) {
            let entry = part.trimmingCharacters(in: .whitespaces)
            if entry.isEmpty { continue }
            if let colon = flowMappingColon(entry) {
                let keyText = String(entry[entry.startIndex..<colon]).trimmingCharacters(in: .whitespaces)
                let valText = String(entry[entry.index(after: colon)...]).trimmingCharacters(in: .whitespaces)
                let value = parseFlow(valText) ?? .null
                entries.append(YamlValue.Pair(unquoteKey(keyText), value))
            } else {
                entries.append(YamlValue.Pair(unquoteKey(entry), .null))
            }
        }
        return .map(entries)
    }

    static func parseFlowSeq(_ s: String) -> YamlValue? {
        guard s.hasPrefix("["), s.hasSuffix("]") else { return nil }
        let inner = String(s.dropFirst().dropLast())
        var items: [YamlValue] = []
        for part in splitTopLevel(inner) {
            let element = part.trimmingCharacters(in: .whitespaces)
            if element.isEmpty { continue }
            items.append(parseFlow(element) ?? .null)
        }
        return .list(items)
    }
}

// MARK: - Shared scalar + scanning helpers

/// Parses a value that may be a flow collection, quoted, or plain scalar.
func scalarFrom(_ s: String) -> YamlValue {
    FlowParser.parseFlow(s) ?? .string(s)
}

/// Parses a scalar leaf (quoted or plain) — not a flow collection.
func scalarLeaf(_ raw: String) -> YamlValue {
    let s = raw.trimmingCharacters(in: .whitespaces)
    if s.isEmpty { return .null }
    if s.hasPrefix("\"") { return .string(unescapeDoubleQuoted(s)) }
    if s.hasPrefix("'") { return .string(unescapeSingleQuoted(s)) }
    return inferPlain(s)
}

/// Core-schema plain scalar type inference.
func inferPlain(_ s: String) -> YamlValue {
    switch s {
    case "null", "Null", "NULL", "~": return .null
    case "true", "True", "TRUE": return .bool(true)
    case "false", "False", "FALSE": return .bool(false)
    default: break
    }
    if isIntLiteral(s), let i = Int(s) { return .int(i) }
    if isFloatLiteral(s), let d = Double(s) { return .double(d) }
    return .string(s)
}

private let intRE = try! NSRegularExpression(pattern: "^[-+]?[0-9]+$")
private let floatRE = try! NSRegularExpression(pattern: "^[-+]?(\\.[0-9]+|[0-9]+(\\.[0-9]*)?)([eE][-+]?[0-9]+)?$")

private func isIntLiteral(_ s: String) -> Bool { matches(intRE, s) }
private func isFloatLiteral(_ s: String) -> Bool {
    // Must look like a float and carry a '.' or exponent (ints handled first).
    guard matches(floatRE, s) else { return false }
    return s.contains(".") || s.lowercased().contains("e")
}

/// Unquotes a mapping key: double/single-quoted → unescaped; else verbatim.
func unquoteKey(_ text: String) -> String {
    let s = text.trimmingCharacters(in: .whitespaces)
    if s.count >= 2, s.hasPrefix("\""), s.hasSuffix("\"") { return unescapeDoubleQuoted(s) }
    if s.count >= 2, s.hasPrefix("'"), s.hasSuffix("'") { return unescapeSingleQuoted(s) }
    return s
}

func unescapeSingleQuoted(_ s: String) -> String {
    let inner = String(s.dropFirst().dropLast())
    return inner.replacingOccurrences(of: "''", with: "'")
}

func unescapeDoubleQuoted(_ s: String) -> String {
    let inner = Array(s.dropFirst().dropLast())
    var out = ""
    var i = 0
    while i < inner.count {
        let ch = inner[i]
        if ch == "\\", i + 1 < inner.count {
            let next = inner[i + 1]
            switch next {
            case "n": out += "\n"
            case "t": out += "\t"
            case "r": out += "\r"
            case "b": out += "\u{08}"
            case "f": out += "\u{0C}"
            case "0": out += "\u{00}"
            case "\"": out += "\""
            case "\\": out += "\\"
            case "/": out += "/"
            case "u":
                if i + 5 < inner.count + 1, i + 6 <= inner.count {
                    let hex = String(inner[(i + 2)..<(i + 6)])
                    if let code = UInt32(hex, radix: 16), let scalar = UnicodeScalar(code) {
                        out.unicodeScalars.append(scalar)
                        i += 6
                        continue
                    }
                }
                out += String(next)
            default:
                out += String(next)
            }
            i += 2
        } else {
            out.append(ch)
            i += 1
        }
    }
    return out
}

/// Index of the block-mapping key/value colon: a `:` followed by a space or the
/// end of the line, at flow-depth 0 and outside quotes. Returns nil if none.
func mappingColon(_ s: String) -> String.Index? {
    let chars = Array(s)
    var inS = false, inD = false, depth = 0
    var i = 0
    while i < chars.count {
        let ch = chars[i]
        if inS {
            if ch == "'" {
                if i + 1 < chars.count, chars[i + 1] == "'" { i += 2; continue }
                inS = false
            }
        } else if inD {
            if ch == "\\" { i += 2; continue }
            if ch == "\"" { inD = false }
        } else {
            switch ch {
            case "'": inS = true
            case "\"": inD = true
            case "{", "[": depth += 1
            case "}", "]": depth -= 1
            case ":":
                if depth == 0, i + 1 == chars.count || chars[i + 1] == " " {
                    return s.index(s.startIndex, offsetBy: i)
                }
            default: break
            }
        }
        i += 1
    }
    return nil
}

/// Like `mappingColon` but for a single flow-map entry: the first `:` at depth 0
/// outside quotes, whether or not a space follows (flow allows `key:value`), but
/// preferring `: ` semantics for values like `B9:B11`.
func flowMappingColon(_ s: String) -> String.Index? {
    let chars = Array(s)
    var inS = false, inD = false, depth = 0
    var i = 0
    while i < chars.count {
        let ch = chars[i]
        if inS {
            if ch == "'" {
                if i + 1 < chars.count, chars[i + 1] == "'" { i += 2; continue }
                inS = false
            }
        } else if inD {
            if ch == "\\" { i += 2; continue }
            if ch == "\"" { inD = false }
        } else {
            switch ch {
            case "'": inS = true
            case "\"": inD = true
            case "{", "[": depth += 1
            case "}", "]": depth -= 1
            case ":":
                if depth == 0, i + 1 == chars.count || chars[i + 1] == " " {
                    return s.index(s.startIndex, offsetBy: i)
                }
            default: break
            }
        }
        i += 1
    }
    return nil
}

/// Splits a flow collection's inner text on top-level commas, respecting quotes
/// and nested `{}`/`[]`.
func splitTopLevel(_ s: String) -> [String] {
    let chars = Array(s)
    var out: [String] = []
    var cur = ""
    var inS = false, inD = false, depth = 0
    var i = 0
    while i < chars.count {
        let ch = chars[i]
        if inS {
            cur.append(ch)
            if ch == "'" {
                if i + 1 < chars.count, chars[i + 1] == "'" { cur.append(chars[i + 1]); i += 2; continue }
                inS = false
            }
        } else if inD {
            cur.append(ch)
            if ch == "\\", i + 1 < chars.count { cur.append(chars[i + 1]); i += 2; continue }
            if ch == "\"" { inD = false }
        } else {
            switch ch {
            case "'": inS = true; cur.append(ch)
            case "\"": inD = true; cur.append(ch)
            case "{", "[": depth += 1; cur.append(ch)
            case "}", "]": depth -= 1; cur.append(ch)
            case ",":
                if depth == 0 { out.append(cur); cur = "" } else { cur.append(ch) }
            default: cur.append(ch)
            }
        }
        i += 1
    }
    out.append(cur)
    return out
}

/// Strips a trailing `# comment` (the `#` preceded by whitespace or at start),
/// outside quotes and flow collections.
func stripComment(_ s: String) -> String {
    let chars = Array(s)
    var inS = false, inD = false, depth = 0
    var i = 0
    while i < chars.count {
        let ch = chars[i]
        if inS {
            if ch == "'" {
                if i + 1 < chars.count, chars[i + 1] == "'" { i += 2; continue }
                inS = false
            }
        } else if inD {
            if ch == "\\" { i += 2; continue }
            if ch == "\"" { inD = false }
        } else {
            switch ch {
            case "'": inS = true
            case "\"": inD = true
            case "{", "[": depth += 1
            case "}", "]": depth -= 1
            case "#":
                if depth == 0, i == 0 || chars[i - 1] == " " {
                    return String(chars[0..<i]).trimmingCharacters(in: CharacterSet(charactersIn: " "))
                }
            default: break
            }
        }
        i += 1
    }
    return s
}
