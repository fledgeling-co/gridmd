// A1-reference parsing (SPEC.md §8.2, Appendix A). Port of js/src/refs.js.

import Foundation

enum Refs {
    static let maxCol = 16384 // XFD
    static let maxRow = 1_048_576

    static func colToNum(_ letters: Substring) -> Int {
        var n = 0
        for ch in letters.unicodeScalars {
            n = n * 26 + (Int(ch.value) - 64)
        }
        return n
    }

    static func numToCol(_ input: Int) -> String {
        var n = input
        var s = ""
        while n > 0 {
            let r = (n - 1) % 26
            s = String(UnicodeScalar(UInt8(65 + r))) + s
            n = (n - 1 - r) / 26
        }
        return s
    }

    struct Cell { let col: Int; let row: Int }

    enum Target {
        case cell(sheet: String?, c1: Int, r1: Int, c2: Int, r2: Int)
        case range(sheet: String?, c1: Int, r1: Int, c2: Int, r2: Int)
        case cols(sheet: String?, c1: Int, c2: Int)
        case rows(sheet: String?, r1: Int, r2: Int)

        var kind: String {
            switch self {
            case .cell: return "cell"
            case .range: return "range"
            case .cols: return "cols"
            case .rows: return "rows"
            }
        }

        var sheet: String? {
            switch self {
            case let .cell(s, _, _, _, _), let .range(s, _, _, _, _): return s
            case let .cols(s, _, _): return s
            case let .rows(s, _, _): return s
            }
        }
    }

    private static let cellRE = try! NSRegularExpression(pattern: "^(\\$?)([A-Z]{1,3})(\\$?)([1-9]\\d{0,6})$")
    private static let colRangeRE = try! NSRegularExpression(pattern: "^\\$?([A-Z]{1,3}):\\$?([A-Z]{1,3})$")
    private static let rowRangeRE = try! NSRegularExpression(pattern: "^\\$?([1-9]\\d{0,6}):\\$?([1-9]\\d{0,6})$")

    static func parseCell(_ text: String) -> Cell? {
        guard let m = firstMatch(cellRE, text) else { return nil }
        let col = colToNum(Substring(m[2]))
        guard let row = Int(m[4]) else { return nil }
        if col > maxCol || row > maxRow { return nil }
        return Cell(col: col, row: row)
    }

    /// Parses a target: cell | cell:cell | col:col | row:row, with an optional
    /// leading `Sheet!` qualifier (quoted names supported).
    static func parseTarget(_ input: String) -> Target? {
        var text = input
        var sheet: String?
        if let bang = text.lastIndex(of: "!") {
            var s = String(text[text.startIndex..<bang])
            if s.hasPrefix("'"), s.hasSuffix("'"), s.count >= 2 {
                s = String(s.dropFirst().dropLast()).replacingOccurrences(of: "''", with: "'")
            }
            sheet = s
            text = String(text[text.index(after: bang)...])
        }

        if let cell = parseCell(text) {
            return .cell(sheet: sheet, c1: cell.col, r1: cell.row, c2: cell.col, r2: cell.row)
        }
        if text.contains(":") {
            let parts = text.split(separator: ":", omittingEmptySubsequences: false).map(String.init)
            if parts.count == 2 {
                if let a = parseCell(parts[0]), let b = parseCell(parts[1]) {
                    return .range(
                        sheet: sheet,
                        c1: min(a.col, b.col), r1: min(a.row, b.row),
                        c2: max(a.col, b.col), r2: max(a.row, b.row)
                    )
                }
                if let m = firstMatch(colRangeRE, text) {
                    let c1 = colToNum(Substring(m[1])), c2 = colToNum(Substring(m[2]))
                    if c1 <= maxCol, c2 <= maxCol {
                        return .cols(sheet: sheet, c1: min(c1, c2), c2: max(c1, c2))
                    }
                }
                if let m = firstMatch(rowRangeRE, text), let r1 = Int(m[1]), let r2 = Int(m[2]) {
                    if r1 <= maxRow, r2 <= maxRow {
                        return .rows(sheet: sheet, r1: min(r1, r2), r2: max(r1, r2))
                    }
                }
            }
        }
        return nil
    }

    static func refKey(_ col: Int, _ row: Int) -> String { "\(col),\(row)" }
}

/// Returns capture groups (0 = whole match) or nil. Empty groups become "".
func firstMatch(_ re: NSRegularExpression, _ text: String) -> [String]? {
    let ns = text as NSString
    guard let m = re.firstMatch(in: text, range: NSRange(location: 0, length: ns.length)) else {
        return nil
    }
    var groups: [String] = []
    for i in 0..<m.numberOfRanges {
        let r = m.range(at: i)
        groups.append(r.location == NSNotFound ? "" : ns.substring(with: r))
    }
    return groups
}
