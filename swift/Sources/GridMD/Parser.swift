// GridMD document parser (SPEC.md §2–§10, Appendix A). Port of js/src/parser.js.
// Produces a block tree; semantic checks live in Validate.swift.

import Foundation

let reservedKinds: Set<String> = [
    "sheet", "grid", "spill-cache", "table", "cf", "validation", "filter",
    "chart", "sparklines", "pivot", "slicer", "image", "shape", "textbox",
    "checkbox", "comments", "outline", "page", "query", "script", "scenario",
    "raw",
]

struct Diagnostic {
    let line: Int
    let msg: String
}

struct InfoArgs {
    var positional: [String] = []
    var flags: [String: String] = [:]
    var anchor: String?
    var size: (w: Int, h: Int)?
}

struct Row {
    let cells: [String]
    let line: Int
}

final class Fence {
    let kind: String
    let args: InfoArgs
    let body: [String]
    let line: Int
    var meta: YamlValue?
    var rows: [Row]?
    var code: String?
    var payload: String?

    init(kind: String, args: InfoArgs, body: [String], line: Int) {
        self.kind = kind
        self.args = args
        self.body = body
        self.line = line
    }
}

final class AtDirective {
    let targetText: String
    let line: Int
    var scalarText: String?
    var props: YamlValue?
    var body: YamlValue?

    init(targetText: String, line: Int) {
        self.targetText = targetText
        self.line = line
    }
}

enum Block {
    case fence(Fence)
    case at(AtDirective)
}

final class SheetNode {
    let name: String
    let line: Int
    var blocks: [Block] = []
    init(name: String, line: Int) {
        self.name = name
        self.line = line
    }
}

final class Document {
    var frontmatter: YamlValue = .map([])
    var workbookBlocks: [Block] = []
    var sheets: [SheetNode] = []
    var errors: [Diagnostic] = []
    var warnings: [Diagnostic] = []
    let mode: String
    var statsDefs = 0
    var statsBlocks = 0

    init(mode: String) { self.mode = mode }
}

enum Parser {
    private static let fenceOpenRE = try! NSRegularExpression(pattern: "^(`{3,})\\{([A-Za-z][A-Za-z0-9-]*)\\}(.*)$")
    private static let sizeRE = try! NSRegularExpression(pattern: "^(\\d+)x(\\d+)$")
    private static let flagRE = try! NSRegularExpression(pattern: "^([A-Za-z][A-Za-z0-9-]*)=(.*)$")
    private static let infoTokenRE = try! NSRegularExpression(pattern: "\"((?:[^\"]|\"\")*)\"|\\S+")
    private static let headingRE = try! NSRegularExpression(pattern: "^# (.+)$")

    static func parseYamlInto(_ text: String, _ line: Int, _ errors: inout [Diagnostic]) -> YamlValue {
        let result = Yaml.parse(text)
        for e in result.errors { errors.append(Diagnostic(line: line, msg: "YAML: \(e)")) }
        return result.value
    }

    /// Right-edge props split (SPEC §9.1 / Appendix A). Port of findPropsSplit.
    static func findPropsSplit(_ text: String) -> (scalarText: String, propsText: String?) {
        if !text.hasSuffix("}") { return (text, nil) }
        let chars = Array(text)
        var inQ = false
        var depth = 0
        var start = -1
        var lastGroup: (Int, Int)?
        for i in 0..<chars.count {
            let ch = chars[i]
            if ch == "\"" { inQ.toggle(); continue }
            if inQ { continue }
            if ch == "{" {
                if depth == 0 { start = i }
                depth += 1
            } else if ch == "}" {
                depth -= 1
                if depth == 0, start != -1 { lastGroup = (start, i) }
                if depth < 0 { return (text, nil) }
            }
        }
        guard let (s, e) = lastGroup else { return (text, nil) }
        if e != chars.count - 1 || s == 0 || chars[s - 1] != " " {
            return (text, nil)
        }
        let scalar = String(chars[0..<s]).replacingOccurrences(of: " +$", with: "", options: .regularExpression)
        return (scalar, String(chars[s...]))
    }

    /// Pipe row → trimmed cell strings; backslash escapes the next character.
    /// Returns nil if the line is not a well-formed pipe row.
    static func splitPipeRow(_ rawLine: String) -> [String]? {
        let line = rawLine.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        let chars = Array(line)
        if !line.hasPrefix("|") || chars.count < 2 { return nil }
        var cells: [String] = []
        var cell = ""
        var opened = false
        var i = 0
        while i < chars.count {
            let ch = chars[i]
            if ch == "\\", i + 1 < chars.count {
                cell.append(chars[i + 1])
                i += 2
                continue
            }
            if ch == "|" {
                if !opened { opened = true; i += 1; continue }
                cells.append(cell.trimmingCharacters(in: .whitespaces))
                cell = ""
                i += 1
                continue
            }
            cell.append(ch)
            i += 1
        }
        if cell.trimmingCharacters(in: .whitespaces) != "" { return nil }
        return cells
    }

    /// Fence info string: positional args, at-anchors, size WxH, key=val flags.
    static func parseInfoArgs(_ rest: String, _ line: Int, _ errors: inout [Diagnostic]) -> InfoArgs {
        var out = InfoArgs()
        let ns = rest as NSString
        var tokens: [(v: String, q: Bool)] = []
        infoTokenRE.enumerateMatches(in: rest, range: NSRange(location: 0, length: ns.length)) { m, _, _ in
            guard let m else { return }
            let g1 = m.range(at: 1)
            if g1.location != NSNotFound {
                tokens.append((ns.substring(with: g1).replacingOccurrences(of: "\"\"", with: "\""), true))
            } else {
                tokens.append((ns.substring(with: m.range(at: 0)), false))
            }
        }
        var k = 0
        while k < tokens.count {
            let tok = tokens[k]
            if !tok.q, tok.v == "at" {
                k += 1
                if k >= tokens.count { errors.append(Diagnostic(line: line, msg: "`at` requires an anchor")); break }
                out.anchor = tokens[k].v
                k += 1
                continue
            }
            if !tok.q, tok.v == "size" {
                k += 1
                if k < tokens.count, let sm = firstMatch(sizeRE, tokens[k].v), let w = Int(sm[1]), let h = Int(sm[2]) {
                    out.size = (w, h)
                } else {
                    errors.append(Diagnostic(line: line, msg: "`size` requires WxH (e.g. 480x320)"))
                }
                k += 1
                continue
            }
            if !tok.q, let fm = firstMatch(flagRE, tok.v) {
                var value = fm[2]
                value = value.replacingOccurrences(of: "^\"(.*)\"$", with: "$1", options: .regularExpression)
                value = value.replacingOccurrences(of: "\"\"", with: "\"")
                out.flags[fm[1]] = value
                k += 1
                continue
            }
            out.positional.append(tok.v)
            k += 1
        }
        return out
    }

    private static func parseFence(_ lines: [String], _ i: Int, _ m: [String], _ errors: inout [Diagnostic]) -> (Fence, Int) {
        let open = m[1].count
        let kind = m[2]
        let args = parseInfoArgs(m[3], i + 1, &errors)
        var body: [String] = []
        var j = i + 1
        var closed = false
        let closeRE = try! NSRegularExpression(pattern: "^`{\(open),}\\s*$")
        while j < lines.count {
            if matches(closeRE, lines[j]) { closed = true; j += 1; break }
            body.append(lines[j])
            j += 1
        }
        if !closed { errors.append(Diagnostic(line: i + 1, msg: "unclosed {\(kind)} fence")) }
        let block = Fence(kind: kind, args: args, body: body, line: i + 1)
        refineFence(block, &errors)
        return (block, j)
    }

    private static func parseRows(_ bodyLines: [String], _ baseLine: Int, _ errors: inout [Diagnostic]) -> [Row] {
        var rows: [Row] = []
        for (k, l) in bodyLines.enumerated() {
            if l.trimmingCharacters(in: .whitespaces).isEmpty { continue }
            if let cells = splitPipeRow(l) {
                rows.append(Row(cells: cells, line: baseLine + k + 1))
            } else {
                errors.append(Diagnostic(line: baseLine + k + 1, msg: "expected a pipe row, got: \(String(l.prefix(50)))"))
            }
        }
        return rows
    }

    private static func refineFence(_ block: Fence, _ errors: inout [Diagnostic]) {
        let kind = block.kind
        let line = block.line
        func meta(_ arr: [String], _ off: Int) -> YamlValue {
            parseYamlInto(arr.joined(separator: "\n"), line + off, &errors)
        }
        if kind == "grid" || kind == "spill-cache" {
            block.rows = parseRows(block.body, line, &errors)
        } else if kind == "table" {
            if let d = block.body.firstIndex(of: "---") {
                block.meta = meta(Array(block.body[0..<d]), 1)
                block.rows = parseRows(Array(block.body[(d + 1)...]), line + d + 1, &errors)
            } else {
                errors.append(Diagnostic(line: line, msg: "{table} requires a `---`-separated payload of pipe rows"))
                block.meta = meta(block.body, 1)
                block.rows = []
            }
        } else if kind == "script" {
            if let d = block.body.firstIndex(of: "---") {
                block.meta = meta(Array(block.body[0..<d]), 1)
                block.code = block.body[(d + 1)...].joined(separator: "\n")
            } else {
                block.meta = .map([])
                block.code = block.body.joined(separator: "\n")
            }
        } else if kind == "raw" || kind.hasPrefix("x-") {
            block.payload = block.body.joined(separator: "\n")
        } else {
            block.meta = meta(block.body, 1)
        }
    }

    private static func parseAt(_ lines: [String], _ i: Int, _ errors: inout [Diagnostic]) -> (AtDirective, Int) {
        let line = lines[i]
        let rest = String(line.dropFirst(2))
        let targetText: String
        let inline: String
        if let sp = rest.firstIndex(of: " ") {
            targetText = String(rest[rest.startIndex..<sp])
            inline = String(rest[rest.index(after: sp)...]).trimmingCharacters(in: .whitespaces)
        } else {
            targetText = rest
            inline = ""
        }

        var j = i + 1
        var lastTake = 0
        var taken = 0
        var acc: [String] = []
        while j < lines.count {
            let l = lines[j]
            if l.trimmingCharacters(in: .whitespaces).isEmpty {
                acc.append("")
                j += 1
                taken += 1
                continue
            }
            if l.hasPrefix("  ") {
                acc.append(String(l.dropFirst(2)))
                j += 1
                taken += 1
                lastTake = taken
                continue
            }
            break
        }
        let bodyLines: [String]? = lastTake > 0 ? Array(acc[0..<lastTake]) : nil
        let next = i + 1 + lastTake

        let block = AtDirective(targetText: targetText, line: i + 1)
        if let bodyLines {
            let parsed = parseYamlInto(bodyLines.joined(separator: "\n"), i + 2, &errors)
            if parsed.isMap {
                block.body = parsed
            } else {
                errors.append(Diagnostic(line: i + 2, msg: "@ directive body must be a YAML mapping"))
            }
        }
        if !inline.isEmpty {
            if inline.hasPrefix("{"), !inline.hasPrefix("{=") {
                if let props = Yaml.tryProps(inline) {
                    block.props = props
                    return (block, next)
                }
            }
            let (scalarText, propsText) = findPropsSplit(inline)
            if let propsText, let props = Yaml.tryProps(propsText) {
                block.props = props
                block.scalarText = scalarText.isEmpty ? nil : scalarText
                return (block, next)
            }
            block.scalarText = inline
        }
        return (block, next)
    }

    static func parseDocument(_ source: String, mode: String = "strict") -> Document {
        let doc = Document(mode: mode)
        let lines = source.components(separatedBy: "\n").map {
            $0.hasSuffix("\r") ? String($0.dropLast()) : $0
        }

        if lines.isEmpty || lines[0] != "---" {
            doc.errors.append(Diagnostic(line: 1, msg: "document must begin with `---` YAML frontmatter"))
            return doc
        }
        var fmEnd = -1
        var k = 1
        while k < lines.count {
            if lines[k] == "---" { fmEnd = k; break }
            k += 1
        }
        if fmEnd == -1 {
            doc.errors.append(Diagnostic(line: 1, msg: "unterminated frontmatter (missing closing `---`)"))
            return doc
        }
        doc.frontmatter = parseYamlInto(lines[1..<fmEnd].joined(separator: "\n"), 2, &doc.errors)

        var i = fmEnd + 1
        var cur: SheetNode?
        func push(_ b: Block) {
            if let cur { cur.blocks.append(b) } else { doc.workbookBlocks.append(b) }
        }

        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty || line.hasPrefix(">") || isSubHeading(line) {
                i += 1
                continue
            }
            if let m = firstMatch(headingRE, line) {
                let node = SheetNode(name: m[1].trimmingCharacters(in: .whitespaces), line: i + 1)
                doc.sheets.append(node)
                cur = node
                i += 1
                continue
            }
            if let m = firstMatch(fenceOpenRE, line) {
                let (block, nextIdx) = parseFence(lines, i, m, &doc.errors)
                push(.fence(block))
                i = nextIdx
                continue
            }
            if line.hasPrefix("@ ") {
                let (block, nextIdx) = parseAt(lines, i, &doc.errors)
                push(.at(block))
                i = nextIdx
                continue
            }
            let diag = Diagnostic(line: i + 1, msg: "unrecognized line: \(String(line.prefix(60)))")
            if mode == "strict" { doc.errors.append(diag) } else { doc.warnings.append(diag) }
            i += 1
        }
        return doc
    }

    /// A heading of level 2+ (`##`, `###`, …) — a doc comment (ignored).
    private static func isSubHeading(_ line: String) -> Bool {
        guard line.hasPrefix("##") else { return false }
        let after = line.drop { $0 == "#" }
        return after.isEmpty || after.first == " "
    }
}
