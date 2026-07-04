// Canonical JSON value + a `JSON.stringify(value, null, 1)`-faithful emitter.
//
// The conformance dump is byte-defined by ECMAScript's
// `JSON.stringify(value, null, 1)`: 1-space indent per depth, `": "` after keys,
// object/array key order = insertion order, control-char escaping exactly as
// `JSON.stringify`, non-ASCII passed through as UTF-8, and no trailing space.

indirect enum JSONValue {
    case null
    case bool(Bool)
    case int(Int)
    case double(Double) // rendered via ESNumber
    case string(String)
    case array([JSONValue])
    case object([(String, JSONValue)]) // insertion-ordered
}

enum JSON {
    /// Emits `value` exactly as `JSON.stringify(value, null, 1)` — no trailing newline.
    static func stringify(_ value: JSONValue) -> String {
        var out = ""
        write(value, indent: 0, into: &out)
        return out
    }

    private static func write(_ value: JSONValue, indent: Int, into out: inout String) {
        switch value {
        case .null:
            out += "null"
        case let .bool(b):
            out += b ? "true" : "false"
        case let .int(i):
            out += String(i)
        case let .double(d):
            out += ESNumber.string(d)
        case let .string(s):
            out += escape(s)
        case let .array(items):
            if items.isEmpty {
                out += "[]"
                return
            }
            let childPad = String(repeating: " ", count: indent + 1)
            out += "[\n"
            for (i, item) in items.enumerated() {
                if i > 0 { out += ",\n" }
                out += childPad
                write(item, indent: indent + 1, into: &out)
            }
            out += "\n" + String(repeating: " ", count: indent) + "]"
        case let .object(entries):
            if entries.isEmpty {
                out += "{}"
                return
            }
            let childPad = String(repeating: " ", count: indent + 1)
            out += "{\n"
            for (i, entry) in entries.enumerated() {
                if i > 0 { out += ",\n" }
                out += childPad + escape(entry.0) + ": "
                write(entry.1, indent: indent + 1, into: &out)
            }
            out += "\n" + String(repeating: " ", count: indent) + "}"
        }
    }

    /// JSON string escaping matching `JSON.stringify`: quote, backslash, the
    /// short control escapes, `\u00XX` (lowercase) for other C0 controls, and
    /// everything else (incl. all non-ASCII) verbatim.
    static func escape(_ s: String) -> String {
        var out = "\""
        for scalar in s.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\u{08}": out += "\\b"
            case "\u{09}": out += "\\t"
            case "\u{0A}": out += "\\n"
            case "\u{0C}": out += "\\f"
            case "\u{0D}": out += "\\r"
            default:
                if scalar.value < 0x20 {
                    out += "\\u" + hex4(scalar.value)
                } else {
                    out.unicodeScalars.append(scalar)
                }
            }
        }
        out += "\""
        return out
    }

    private static func hex4(_ v: UInt32) -> String {
        let digits = Array("0123456789abcdef")
        var s = ""
        for shift in stride(from: 12, through: 0, by: -4) {
            s.append(digits[Int((v >> UInt32(shift)) & 0xF)])
        }
        return s
    }
}
