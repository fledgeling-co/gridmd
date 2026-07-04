// Cell scalar micro-grammar (SPEC.md §6). Port of js/src/scalar.js.

import Foundation

/// A parsed cell scalar. Mirrors the shape of the JS reference's plain objects
/// (reference type so a formula's `cached` can be filled in later, as the model
/// builder does when a `{spill-cache}` supplies a formula's cached value).
final class Scalar {
    var kind: String // blank | text | number | boolean | date | time | error | formula | invalid
    var stringValue: String?
    var numberValue: Double?
    var boolValue: Bool?
    var formula: String?
    var cse = false
    var cached: Scalar?
    var problem: String?
    var forced = false
    var quoted = false
    var spillCache = false // model-side marker (not part of the grammar)

    init(kind: String) { self.kind = kind }

    static func blank() -> Scalar { Scalar(kind: "blank") }
    static func text(_ v: String) -> Scalar { let s = Scalar(kind: "text"); s.stringValue = v; return s }
    static func number(_ v: Double) -> Scalar { let s = Scalar(kind: "number"); s.numberValue = v; return s }
    static func boolean(_ v: Bool) -> Scalar { let s = Scalar(kind: "boolean"); s.boolValue = v; return s }
    static func date(_ v: String) -> Scalar { let s = Scalar(kind: "date"); s.stringValue = v; return s }
    static func time(_ v: String) -> Scalar { let s = Scalar(kind: "time"); s.stringValue = v; return s }
    static func error(_ v: String) -> Scalar { let s = Scalar(kind: "error"); s.stringValue = v; return s }
}

enum ScalarGrammar {
    static let errorValues: Set<String> = [
        "#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A",
        "#GETTING_DATA", "#SPILL!", "#CALC!", "#FIELD!", "#BLOCKED!",
    ]

    private static let numberRE = try! NSRegularExpression(pattern: "^-?(0|[1-9]\\d*)(\\.\\d+)?([eE][+-]?\\d+)?$")
    private static let dateRE = try! NSRegularExpression(pattern: "^\\d{4}-\\d{2}-\\d{2}(T\\d{2}:\\d{2}(:\\d{2})?)?$")
    private static let timeRE = try! NSRegularExpression(pattern: "^\\d{2}:\\d{2}(:\\d{2})?$")
    private static let dqRE = try! NSRegularExpression(pattern: "^\"((?:[^\"]|\"\")*)\"$")

    /// Splits "formula :: cached" at the LAST " :: " outside double-quoted string
    /// literals (SPEC §6). Returns (head, cached) with cached nil if none.
    static func splitCached(_ text: String) -> (head: String, cached: String?) {
        let chars = Array(text)
        var inQ = false
        var idx = -1
        var i = 0
        while i < chars.count {
            let ch = chars[i]
            if ch == "\"" {
                inQ.toggle()
                i += 1
                continue
            }
            if !inQ, ch == " ", startsWith(chars, i, [" ", ":", ":", " "]) {
                idx = i
            }
            i += 1
        }
        if idx == -1 { return (text, nil) }
        let head = String(chars[0..<idx])
        let cached = String(chars[(idx + 4)...]).trimmingCharacters(in: .whitespaces)
        return (head, cached)
    }

    private static func startsWith(_ chars: [Character], _ at: Int, _ needle: [Character]) -> Bool {
        if at + needle.count > chars.count { return false }
        for j in 0..<needle.count where chars[at + j] != needle[j] { return false }
        return true
    }

    /// Parses one cell scalar. `raw` must already be trimmed.
    static func parseScalar(_ raw: String) -> Scalar {
        if raw.isEmpty { return .blank() }

        if raw.hasPrefix("{=") {
            let (head, cached) = splitCached(raw)
            if !head.hasSuffix("}") {
                let s = Scalar.text(raw)
                s.problem = "unterminated CSE array formula"
                return s
            }
            let s = Scalar(kind: "formula")
            s.cse = true
            s.formula = String(head.dropFirst(2).dropLast())
            s.cached = parseCached(cached)
            return s
        }
        if raw.hasPrefix("=") {
            let (head, cached) = splitCached(raw)
            let s = Scalar(kind: "formula")
            s.cse = false
            s.formula = String(head.dropFirst())
            s.cached = parseCached(cached)
            return s
        }
        if raw.hasPrefix("'") {
            let s = Scalar.text(String(raw.dropFirst()))
            s.forced = true
            return s
        }
        if raw.hasPrefix("\"") {
            if let m = firstMatch(dqRE, raw) {
                let s = Scalar.text(m[1].replacingOccurrences(of: "\"\"", with: "\""))
                s.quoted = true
                return s
            }
            let s = Scalar.text(raw)
            s.problem = "unterminated quoted text"
            return s
        }
        if matches(numberRE, raw), let d = Double(raw) {
            return .number(d)
        }
        let up = raw.uppercased()
        if up == "TRUE" || up == "FALSE" { return .boolean(up == "TRUE") }
        if matches(dateRE, raw) { return .date(raw) }
        if matches(timeRE, raw) { return .time(raw) }
        if errorValues.contains(up) { return .error(up) }
        return .text(raw)
    }

    private static func parseCached(_ cachedText: String?) -> Scalar? {
        guard let cachedText else { return nil }
        let v = parseScalar(cachedText)
        if v.kind == "formula" {
            let inv = Scalar(kind: "invalid")
            inv.problem = "cached value must not be a formula"
            return inv
        }
        return v
    }
}

func matches(_ re: NSRegularExpression, _ text: String) -> Bool {
    let ns = text as NSString
    return re.firstMatch(in: text, range: NSRange(location: 0, length: ns.length)) != nil
}
