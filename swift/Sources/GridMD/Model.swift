// Materializes a parsed GridMD document into the per-sheet workbook model the
// dump reads (js/src/xlsx/model.js — the fields dump.js consumes): effective
// cells (with `{spill-cache}` cached-fill and set-once semantics), merges,
// tables, and the count-bearing feature collections. Formatting patches, the
// carry part, raw parts and the fidelity report are computed by the XLSX writer,
// not needed by the dump, and are omitted here.

import Foundation

private let dateRE = try! NSRegularExpression(pattern: "^\\d{4}-\\d{2}-\\d{2}(T\\d{2}:\\d{2}(:\\d{2})?)?$")
private let timeRE = try! NSRegularExpression(pattern: "^\\d{2}:\\d{2}(:\\d{2})?$")

final class CellContent {
    var formula: String?
    var cse = false
    var cached: Scalar?
    var hasCached = false // JS: whether the content's `cached` field is defined
    var arrayRef: String?
    var scalar: Scalar?
    var rich: [YamlValue]?
    var spillCache = false
}

final class ModelCell {
    let col: Int
    let row: Int
    var content: CellContent?
    init(col: Int, row: Int) { self.col = col; self.row = row }
}

struct ModelTable {
    let name: String
    let anchor: Refs.Cell
    let columns: [String]
    let bodyRows: Int
    let hasTotals: Bool
}

final class ModelSheet {
    let name: String
    let meta: YamlValue
    let kind: String
    var cellsByKey: [String: ModelCell] = [:]
    var merges: [(c1: Int, r1: Int, c2: Int, r2: Int)] = []
    var tables: [ModelTable] = []
    var cfRuleCounts: [Int] = []
    var validations = 0
    var notes = 0
    var threads = 0
    var scenarios = 0
    var sparklines = 0
    var charts = 0
    var pivots = 0
    var slicers = 0
    var images = 0
    var shapes = 0
    var hyperlinks = 0

    init(name: String, meta: YamlValue, kind: String) {
        self.name = name
        self.meta = meta
        self.kind = kind
    }

    func cellAt(_ col: Int, _ row: Int) -> ModelCell {
        let k = Refs.refKey(col, row)
        if let c = cellsByKey[k] { return c }
        let c = ModelCell(col: col, row: row)
        cellsByKey[k] = c
        return c
    }

    func setContent(_ col: Int, _ row: Int, _ content: CellContent) {
        let c = cellAt(col, row)
        if c.content == nil {
            c.content = content
        } else if content.hasCached, let existing = c.content, existing.formula != nil, existing.cached == nil {
            existing.cached = content.cached
        }
    }
}

struct WorkbookModel {
    let fm: YamlValue
    let sheets: [ModelSheet]
}

enum Model {
    static func build(_ doc: Document) -> WorkbookModel {
        var sheets: [ModelSheet] = []
        for sheet in doc.sheets {
            var sheetMeta: YamlValue = .map([])
            for block in sheet.blocks {
                if case let .fence(f) = block, f.kind == "sheet" { sheetMeta = f.meta ?? .map([]); break }
            }
            let kind = sheetMeta["kind"]?.asString == "chart" ? "chart" : "worksheet"
            let s = ModelSheet(name: sheet.name, meta: sheetMeta, kind: kind)
            sheets.append(s)

            for block in sheet.blocks {
                switch block {
                case .at(let a):
                    applyAt(s, a)
                case .fence(let b):
                    applyFence(s, b)
                }
            }
        }
        return WorkbookModel(fm: doc.frontmatter, sheets: sheets)
    }

    private static func scalarContent(_ sc: Scalar) -> CellContent {
        let c = CellContent()
        if sc.kind == "formula" {
            c.formula = sc.formula
            c.cse = sc.cse
            c.cached = sc.cached
            c.hasCached = true
        } else {
            c.scalar = sc
        }
        return c
    }

    private static func applyFence(_ s: ModelSheet, _ b: Fence) {
        switch b.kind {
        case "sheet":
            break
        case "grid":
            guard let a = Refs.parseCell(b.args.positional.first ?? "") else { break }
            for (ri, row) in (b.rows ?? []).enumerated() {
                for (ci, text) in row.cells.enumerated() {
                    let sc = ScalarGrammar.parseScalar(text)
                    if sc.kind != "blank" { s.setContent(a.col + ci, a.row + ri, scalarContent(sc)) }
                }
            }
        case "spill-cache":
            guard let a = Refs.parseCell(b.args.positional.first ?? "") else { break }
            for (ri, row) in (b.rows ?? []).enumerated() {
                for (ci, text) in row.cells.enumerated() {
                    let sc = ScalarGrammar.parseScalar(text)
                    if sc.kind == "blank" { continue }
                    if ri == 0, ci == 0 {
                        let c = CellContent()
                        c.cached = sc
                        c.hasCached = true
                        s.setContent(a.col, a.row, c)
                    } else {
                        let c = CellContent()
                        c.scalar = sc
                        c.spillCache = true
                        s.setContent(a.col + ci, a.row + ri, c)
                    }
                }
            }
        case "table":
            applyTable(s, b)
        case "cf":
            s.cfRuleCounts.append(b.meta?.isList == true ? b.meta!.count : 0)
        case "validation":
            s.validations += 1
        case "chart":
            s.charts += 1
        case "sparklines":
            s.sparklines += 1
        case "pivot":
            s.pivots += 1
        case "slicer":
            s.slicers += 1
        case "image":
            s.images += 1
        case "shape", "textbox":
            s.shapes += 1
        case "comments":
            s.threads += 1
        case "scenario":
            s.scenarios += 1
        default:
            // filter/outline/page/checkbox/query/script/raw: no dump-visible count.
            break
        }
    }

    private static func applyTable(_ s: ModelSheet, _ b: Fence) {
        guard let a = Refs.parseCell(b.args.anchor ?? "") else { return }
        let tm = b.meta ?? .map([])
        let header = tm["header"]?.asBool != false
        let rows = b.rows ?? []
        var columns: [String] = []
        for (ri, row) in rows.enumerated() {
            for (ci, text) in row.cells.enumerated() {
                let sc = ScalarGrammar.parseScalar(text)
                if header, ri == 0, sc.kind == "text" { columns.append(sc.stringValue ?? "") }
                if sc.kind != "blank" { s.setContent(a.col + ci, a.row + ri, scalarContent(sc)) }
            }
        }
        if let total = tm["total"], total.isMap {
            let totalRow = a.row + rows.count
            for entry in total.mapEntries {
                guard let ci = columns.firstIndex(where: { $0.lowercased() == entry.key.lowercased() }) else { continue }
                let sc = ScalarGrammar.parseScalar(entry.value.jsString)
                s.setContent(a.col + ci, totalRow, scalarContent(sc))
            }
        }
        s.tables.append(ModelTable(
            name: b.args.positional.first ?? "",
            anchor: a,
            columns: columns,
            bodyRows: rows.count - (header ? 1 : 0),
            hasTotals: tm["total"]?.isTruthy ?? false
        ))
    }

    private static func applyAt(_ s: ModelSheet, _ b: AtDirective) {
        guard let t = Refs.parseTarget(b.targetText), let box = t.box else {
            // whole-row/col @ targets carry no content or dump-visible annotation.
            return
        }
        let body = b.body ?? .map([])
        let flow = b.props ?? .map([])
        func propVal(_ key: String) -> YamlValue? { body[key] ?? flow[key] }

        if let scalarText = b.scalarText {
            let sc = ScalarGrammar.parseScalar(scalarText)
            if t.kind == "cell", sc.kind != "blank" {
                let content = scalarContent(sc)
                if content.formula != nil {
                    let spill = flow["spill"] ?? body["spill"]
                    let arr = flow["array"] ?? body["array"]
                    if let v = spill ?? arr { content.arrayRef = v.jsString }
                    if arr != nil { content.cse = true }
                }
                s.setContent(box.c1, box.r1, content)
            } else if t.kind == "range", sc.kind == "formula" {
                for r in box.r1...box.r2 {
                    for c in box.c1...box.c2 {
                        let content = CellContent()
                        content.formula = translateFormula(sc.formula ?? "", r - box.r1, c - box.c1)
                        content.cse = false
                        content.cached = nil
                        content.hasCached = true
                        s.setContent(c, r, content)
                    }
                }
            }
        } else if let content = bodyContent(body, flow), t.kind == "cell" {
            s.setContent(box.c1, box.r1, content)
        }

        if propVal("merge") == .bool(true), t.kind == "range" {
            s.merges.append((box.c1, box.r1, box.c2, box.r2))
        }
        if let link = propVal("link"), link.isTruthy {
            s.hyperlinks += 1
        }
        if let note = propVal("note"), note.isTruthy {
            s.notes += 1
        }
    }

    private static func bodyContent(_ body: YamlValue, _ flow: YamlValue) -> CellContent? {
        if let f = body["formula"] {
            let content = CellContent()
            content.formula = f.jsString.replacingOccurrences(of: "^=", with: "", options: .regularExpression)
            content.cse = false
            content.cached = body.has("value") ? yamlScalar(body["value"]!) : nil
            content.hasCached = true
            let spill = body["spill"] ?? flow["spill"]
            let arr = body["array"] ?? flow["array"]
            if let v = spill ?? arr { content.arrayRef = v.jsString }
            if arr != nil { content.cse = true }
            return content
        }
        if let rich = body["rich"], rich.isList {
            let content = CellContent()
            content.rich = rich.listItems
            return content
        }
        if let entity = body["entity"] {
            let content = CellContent()
            let text = entity["text"]?.asString ?? entity["id"]?.asString ?? ""
            content.scalar = Scalar.text(text)
            return content
        }
        if let value = body["value"] {
            let content = CellContent()
            content.scalar = yamlScalar(value)
            return content
        }
        return nil
    }

    private static func yamlScalar(_ v: YamlValue) -> Scalar {
        switch v {
        case let .int(i): return Scalar.number(Double(i))
        case let .double(d): return Scalar.number(d)
        case let .bool(b): return Scalar.boolean(b)
        default:
            let str = v.jsString
            if matches(dateRE, str) { return Scalar.date(str) }
            if matches(timeRE, str) { return Scalar.time(str) }
            return Scalar.text(str)
        }
    }
}

/// Relative fill (SPEC §8.5): shift unanchored A1 refs by (dr, dc), skipping
/// string literals and quoted sheet names. Port of model.js translateFormula.
func translateFormula(_ formula: String, _ dr: Int, _ dc: Int) -> String {
    let chars = Array(formula)
    var out = ""
    var i = 0
    let refRE = try! NSRegularExpression(pattern: "^(\\$?)([A-Z]{1,3})(\\$?)(\\d{1,7})(?![A-Za-z0-9_(])")
    let prevRE = try! NSRegularExpression(pattern: "[A-Za-z0-9_.]")
    while i < chars.count {
        let ch = chars[i]
        if ch == "\"" || ch == "'" {
            let q = ch
            var j = i + 1
            while j < chars.count {
                if chars[j] == q {
                    if j + 1 < chars.count, chars[j + 1] == q { j += 2; continue }
                    break
                }
                j += 1
            }
            out += String(chars[i...min(j, chars.count - 1)])
            i = j + 1
            continue
        }
        let rest = String(chars[i...])
        let prev = out.isEmpty ? "" : String(out.last!)
        if let m = firstMatch(refRE, rest), prev.isEmpty || !matches(prevRE, prev) {
            let cd = m[1], colL = m[2], rd = m[3], rowS = m[4]
            let col = cd == "$" ? colL : Refs.numToCol(max(1, Refs.colToNum(Substring(colL)) + dc))
            let row = rd == "$" ? rowS : String(max(1, (Int(rowS) ?? 0) + dr))
            out += "\(cd)\(col)\(rd)\(row)"
            i += m[0].count
            continue
        }
        out.append(ch)
        i += 1
    }
    return out
}
