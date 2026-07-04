// GridMD semantic validation (SPEC.md §9.4, §12–§13; DIRECTIVES.md).
// Port of js/src/validate.js — a single function with nested closures, mirroring
// the reference's structure so the two stay auditable side by side.

import Foundation

private let sheetNameBadRE = try! NSRegularExpression(pattern: "[:\\\\/?*\\[\\]]")
private let tableNameRE = try! NSRegularExpression(pattern: "^[A-Za-z_\\\\][A-Za-z0-9_.\\\\]{0,254}$")
private let cellishNameRE = try! NSRegularExpression(pattern: "^[A-Za-z]{1,3}\\d+$")
private let colorRE = try! NSRegularExpression(pattern: "^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$")
private let themeColorRE = try! NSRegularExpression(pattern: "^(dk1|lt1|dk2|lt2|accent[1-6]|hlink|folHlink)(@-?\\d{1,3})?$")
private let themeSlotRE = try! NSRegularExpression(pattern: "^(dk1|lt1|dk2|lt2|accent[1-6]|hlink|folHlink)$")
private let gridmdVersionRE = try! NSRegularExpression(pattern: "^\\d+\\.\\d+$")
private let colLetterRE = try! NSRegularExpression(pattern: "^[A-Z]{1,3}$")
private let colRangeKeyRE = try! NSRegularExpression(pattern: "^[A-Z]{1,3}(:[A-Z]{1,3})?$")
private let rowKeyRE = try! NSRegularExpression(pattern: "^\\d+(:\\d+)?$")
private let outlineRowRE = try! NSRegularExpression(pattern: "^\\d+:\\d+$")
private let outlineColRE = try! NSRegularExpression(pattern: "^[A-Z]{1,3}:[A-Z]{1,3}$")
private let safeLinkRE = try! NSRegularExpression(pattern: "^(https://|mailto:|#)")
private let jsFileRE = try! NSRegularExpression(pattern: "^(javascript|vbscript|file):", options: .caseInsensitive)
private let dataRE = try! NSRegularExpression(pattern: "^data:", options: .caseInsensitive)
private let dataImageRE = try! NSRegularExpression(pattern: "^data:image/", options: .caseInsensitive)
private let schemeRE = try! NSRegularExpression(pattern: "^[a-z][a-z0-9+.-]*:", options: .caseInsensitive)
private let httpsRE = try! NSRegularExpression(pattern: "^https:", options: .caseInsensitive)
private let partControlRE = try! NSRegularExpression(pattern: "[\\x00-\\x1f ]")
private let partPercentRE = try! NSRegularExpression(pattern: "%2e|%2f|%5c", options: .caseInsensitive)

private let workbookKinds: Set<String> = ["query", "script", "raw"]
private let contentKeys = ["value", "formula", "rich", "entity"]
private let fillEnumerationCap = 10000

private let knownProps: Set<String> = [
    "style", "font", "size", "bold", "italic", "underline", "strike", "sub",
    "super", "color", "fill", "pattern", "fill2", "border", "border-top",
    "border-right", "border-bottom", "border-left", "border-diag-up",
    "border-diag-down", "border-inner", "border-inner-h", "border-inner-v",
    "align", "valign", "rotation", "indent", "wrap", "shrink", "numfmt",
    "merge", "locked", "hidden", "link", "tip", "note", "rich", "spill",
    "array", "control", "entity", "fields", "value", "formula",
]

private let sheetMetaKeys: Set<String> = [
    "kind", "tab-color", "hidden", "freeze", "split", "view",
    "default-row-height", "default-col-width", "cols", "rows", "protect", "names",
]

private let frontmatterKeys: Set<String> = [
    "gridmd", "title", "properties", "locale", "date-system", "calc", "theme",
    "names", "styles", "table-styles", "links", "protection",
]

private let chartTypes: Set<String> = [
    "column", "bar", "line", "area", "pie", "doughnut", "scatter", "bubble",
    "radar", "stock", "surface", "histogram", "pareto", "box-whisker",
    "treemap", "sunburst", "waterfall", "funnel", "map", "combo",
]

private let shapeKinds: Set<String> = [
    "rect", "rounded-rect", "ellipse", "triangle", "right-triangle", "diamond",
    "pentagon", "hexagon", "star", "arrow-right", "arrow-left", "arrow-up",
    "arrow-down", "chevron", "callout", "line", "connector",
]

private let validationTypes = ["list", "whole", "decimal", "date", "time", "text-length", "custom"]
private let cfRuleKeys = ["when", "contains", "not-contains", "begins", "ends", "date", "dupes", "unique", "top", "bottom", "avg", "bars", "scale", "icons", "formula"]

private func chartBaseType(_ t: String) -> String {
    var base = t
    for suf in ["-stacked100", "-stacked", "-3d"] where base.hasSuffix(suf) {
        base = String(base.dropLast(suf.count))
    }
    return base
}

private func isColorString(_ s: String) -> Bool { s == "auto" || matches(colorRE, s) || matches(themeColorRE, s) }
private func isColorValue(_ v: YamlValue?) -> Bool { guard let s = v?.asString else { return false }; return isColorString(s) }
private func isSafeLink(_ v: YamlValue?) -> Bool { guard let s = v?.asString else { return false }; return matches(safeLinkRE, s) }

private func isSafeImageSrc(_ v: String) -> Bool {
    if matches(jsFileRE, v) { return false }
    if matches(dataRE, v) { return matches(dataImageRE, v) }
    if matches(schemeRE, v) { return matches(httpsRE, v) }
    return true
}

/// {raw} part= path rules (DIRECTIVES §18).
func isValidPartPath(_ p: String) -> Bool {
    if p.isEmpty { return false }
    if p.hasPrefix("/") || p.contains("\\") { return false }
    if matches(partControlRE, p) { return false }
    if matches(partPercentRE, p) { return false }
    return p.components(separatedBy: "/").allSatisfy { $0 != "" && $0 != "." && $0 != ".." }
}

struct SheetCtx {
    let target: (String?, Int, [String], String) -> Refs.Target?
    let addDef: (Int, Int, Int, String) -> Void
}

extension Refs.Target {
    /// Bounding box for `cell`/`range` targets (nil for whole-col/row targets).
    var box: (c1: Int, r1: Int, c2: Int, r2: Int)? {
        switch self {
        case let .cell(_, a, b, c, d), let .range(_, a, b, c, d): return (a, b, c, d)
        default: return nil
        }
    }
}

func validateDocument(_ doc: Document) {
    func err(_ line: Int, _ msg: String) { doc.errors.append(Diagnostic(line: line, msg: msg)) }
    func warn(_ line: Int, _ msg: String) { doc.warnings.append(Diagnostic(line: line, msg: msg)) }
    doc.statsDefs = 0
    doc.statsBlocks = 0

    var globalNames: [String: String] = [:]

    // ---- frontmatter ----
    let fm = doc.frontmatter
    if let g = fm["gridmd"]?.asString, matches(gridmdVersionRE, g) {} else {
        err(2, "frontmatter requires gridmd: \"MAJOR.MINOR\" (quoted string)")
    }
    for entry in fm.mapEntries where !frontmatterKeys.contains(entry.key) && !entry.key.hasPrefix("x-") {
        warn(2, "unknown frontmatter key: \(entry.key)")
    }
    if let ds = fm["date-system"], !(ds.asInt == 1900 || ds.asInt == 1904) {
        err(2, "date-system must be 1900 or 1904")
    }
    if let mode = fm["calc"]?["mode"], !["auto", "auto-no-tables", "manual"].contains(mode.asString ?? "") {
        err(2, "calc.mode must be auto | auto-no-tables | manual, got \(mode.jsString)")
    }
    for n in (fm["names"]?.listItems ?? []) {
        guard n.isMap, let nm = n["name"]?.asString else { err(2, "names entries require a name"); continue }
        let forms = ["ref", "formula", "value"].filter { n.has($0) }
        if forms.count != 1 { err(2, "name \(nm): exactly one of ref | formula | value required") }
        if globalNames[nm.lowercased()] != nil { err(2, "duplicate defined name: \(nm)") }
        globalNames[nm.lowercased()] = "name"
    }
    for entry in (fm["styles"]?.mapEntries ?? []) where !entry.value.isMap {
        err(2, "style \(entry.key) must be a mapping")
    }
    for entry in (fm["theme"]?["colors"]?.mapEntries ?? []) {
        if !matches(themeSlotRE, entry.key) { warn(2, "unknown theme color slot: \(entry.key)") }
        else if !matches(colorRE, entry.value.jsString) { err(2, "theme color \(entry.key) must be #RRGGBB") }
    }

    // ---- shared fence validation ----
    func validateFence(_ b: Fence, _ ctx: SheetCtx?) {
        let meta = b.meta ?? .map([])
        let pos = b.args.positional
        func need(_ cond: Bool, _ msg: String) { if !cond { err(b.line, "{\(b.kind)} \(msg)") } }

        switch b.kind {
        case "grid":
            guard let anchor = Refs.parseCell(pos.first ?? "") else { need(false, "requires a cell anchor"); break }
            for (ri, row) in (b.rows ?? []).enumerated() {
                for (ci, cellText) in row.cells.enumerated() {
                    let s = ScalarGrammar.parseScalar(cellText)
                    if let p = s.problem { err(row.line, "grid cell: \(p)") }
                    if s.kind != "blank" { ctx?.addDef(anchor.col + ci, anchor.row + ri, row.line, "{grid}") }
                }
            }
        case "table":
            let name = pos.first
            need(name != nil && matches(tableNameRE, name ?? "") && !matches(cellishNameRE, name ?? ""), "requires a valid table name")
            let anchor = Refs.parseCell(b.args.anchor ?? "")
            need(anchor != nil, "requires `at <cell>`")
            if let name {
                if globalNames[name.lowercased()] != nil { err(b.line, "table name collides with an existing name: \(name)") }
                globalNames[name.lowercased()] = "table"
            }
            let rows = b.rows ?? []
            guard let anchor, !rows.isEmpty else { need(!rows.isEmpty, "requires payload rows"); break }
            let header = meta["header"]?.asBool != false
            var columns: [String] = []
            for (ri, row) in rows.enumerated() {
                for (ci, cellText) in row.cells.enumerated() {
                    let s = ScalarGrammar.parseScalar(cellText)
                    if let p = s.problem { err(row.line, "table cell: \(p)") }
                    if header, ri == 0 {
                        if s.kind != "text" || s.stringValue == "" {
                            err(row.line, "table header cells must be non-empty text (column \(ci + 1))")
                        } else {
                            columns.append(s.stringValue ?? "")
                        }
                        ctx?.addDef(anchor.col + ci, anchor.row + ri, row.line, "{table} header")
                        continue
                    }
                    if s.kind != "blank" { ctx?.addDef(anchor.col + ci, anchor.row + ri, row.line, "{table}") }
                }
            }
            let lower = columns.map { $0.lowercased() }
            for (i, c) in lower.enumerated() where lower.firstIndex(of: c) != i {
                err(b.line, "duplicate table column name: \(columns[i])")
            }
            let colSet = Set(lower)
            func checkCols(_ obj: YamlValue?, _ what: String) {
                for entry in (obj?.mapEntries ?? []) where !colSet.contains(entry.key.lowercased()) {
                    err(b.line, "\(what) references unknown column: \(entry.key)")
                }
            }
            checkCols(meta["cols"], "cols")
            checkCols(meta["total"], "total")
            checkCols(meta["filter"], "filter")
            for s in (meta["sort"]?.listItems ?? []) where !colSet.contains((s["col"]?.asString ?? "").lowercased()) {
                err(b.line, "sort references unknown column: \(s["col"]?.jsString ?? "")")
            }
            if let total = meta["total"], total.isMap {
                let totalRow = anchor.row + rows.count
                for entry in total.mapEntries {
                    if let ci = lower.firstIndex(of: entry.key.lowercased()) {
                        ctx?.addDef(anchor.col + ci, totalRow, b.line, "{table} total")
                    }
                }
            }
        case "cf":
            need(ctx?.target(pos.first, b.line, ["cell", "range", "cols", "rows"], "{cf}") != nil, "requires a target range")
            let rules: [YamlValue]? = meta.isList ? meta.listItems : nil
            need(rules != nil, "body must be a YAML list of rules")
            for rule in rules ?? [] {
                let kinds = cfRuleKeys.filter { rule.has($0) }
                if kinds.count != 1 { err(b.line, "each cf rule needs exactly one distinguishing key") }
                if let pr = rule["priority"], !((pr.asInt ?? 0) >= 1 && pr.asInt != nil) {
                    err(b.line, "cf priority must be a positive integer")
                }
                for key in ["fill", "color"] {
                    if let fv = rule["format"]?[key], !isColorValue(fv) { err(b.line, "cf format.\(key): not a color: \(fv.jsString)") }
                }
            }
        case "validation":
            need(ctx?.target(pos.first, b.line, ["cell", "range", "cols", "rows"], "{validation}") != nil, "requires a target")
            need(validationTypes.contains(meta["type"]?.asString ?? ""), "type must be one of \(validationTypes.joined(separator: " | "))")
            if meta["type"]?.asString == "list" { need(meta.has("values") || meta.has("source"), "list validation requires values: or source:") }
            if let style = meta["error"]?["style"] { need(["stop", "warning", "information"].contains(style.asString ?? ""), "error.style must be stop | warning | information") }
        case "filter":
            need(ctx?.target(pos.first, b.line, ["range"], "{filter}") != nil, "requires a range")
            for entry in (meta["cols"]?.mapEntries ?? []) where !matches(colLetterRE, entry.key) {
                err(b.line, "filter cols keys are column letters on plain ranges: \(entry.key)")
            }
        case "chart":
            let type = pos.first
            if let type, !chartTypes.contains(chartBaseType(type)) { warn(b.line, "unknown chart type \(type) — a converter must carry it via fallback:") }
            need(b.args.anchor != nil, "requires `at <anchor>` (or `at sheet` on a chart sheet)")
            if let anchor = b.args.anchor, anchor != "sheet" { _ = ctx?.target(anchor, b.line, ["cell", "range"], "{chart} at") }
            need(meta.has("series") || meta.has("data") || meta.has("pivot"), "requires series:, data:, or pivot:")
            let series = meta["series"]?.isList == true ? meta["series"]!.listItems : []
            for (i, s) in series.enumerated() {
                if !s.isMap || (!s.has("val") && !meta.has("pivot")) { err(b.line, "series[\(i)] requires val:") }
                if let c = s["color"], !isColorValue(c) { err(b.line, "series[\(i)].color: not a color") }
            }
        case "sparklines":
            need(ctx?.target(pos.first, b.line, ["cell", "range"], "{sparklines}") != nil, "requires a target range")
            need(meta.has("source"), "requires source:")
            if let t = meta["type"] { need(["line", "column", "win-loss"].contains(t.asString ?? ""), "type must be line | column | win-loss") }
        case "pivot":
            need(pos.first != nil, "requires a name")
            let anchorText = (b.args.anchor ?? "").replacingOccurrences(of: "^.*!", with: "", options: .regularExpression)
            need(Refs.parseCell(anchorText) != nil, "requires `at <cell>`")
            need(meta.has("source"), "requires source:")
            if let name = pos.first {
                if globalNames[name.lowercased()] != nil { err(b.line, "pivot name collides with an existing name: \(name)") }
                globalNames[name.lowercased()] = "pivot"
            }
        case "slicer":
            need(b.args.anchor != nil, "requires an anchor")
            need(meta.has("for") && meta.has("field"), "requires for: and field:")
        case "image":
            need(b.args.anchor != nil, "requires an anchor")
            need(meta["src"]?.asString != nil, "requires src:")
            if let src = meta["src"]?.asString, !isSafeImageSrc(src) { err(b.line, "image src fails the scheme allowlist: \(src)") }
        case "shape":
            if let p = pos.first, !shapeKinds.contains(p) { warn(b.line, "unknown shape kind \(p) — carry exotic geometry via fallback:") }
            need(b.args.anchor != nil, "requires an anchor")
        case "textbox":
            need(b.args.anchor != nil, "requires an anchor")
        case "checkbox":
            need(b.args.anchor != nil, "requires an anchor")
            if let linked = meta["linked"] { need(Refs.parseCell(linked.jsString.replacingOccurrences(of: "$", with: "")) != nil, "linked: must be a cell") }
        case "comments":
            need(ctx?.target(pos.first, b.line, ["cell"], "{comments}") != nil, "requires a cell target")
            let list: [YamlValue]? = meta.isList ? meta.listItems : nil
            need(list != nil, "body must be a YAML list of comments")
            for c in list ?? [] where !c.has("by") || !c.has("at") || !c.has("text") { err(b.line, "each comment requires by:, at:, text:") }
        case "outline":
            for r in (meta["rows"]?.listItems ?? []) where !matches(outlineRowRE, r["range"]?.jsString ?? "") {
                err(b.line, "outline rows range must be \"n:m\": \(r["range"]?.jsString ?? "")")
            }
            for c in (meta["cols"]?.listItems ?? []) where !matches(outlineColRE, c["range"]?.jsString ?? "") {
                err(b.line, "outline cols range must be \"A:B\": \(c["range"]?.jsString ?? "")")
            }
        case "page":
            if let o = meta["orientation"] { need(["portrait", "landscape"].contains(o.asString ?? ""), "orientation must be portrait | landscape") }
            need(!(meta.has("scale") && meta.has("fit")), "scale: and fit: are mutually exclusive")
        case "query":
            need(pos.first != nil, "requires a name")
            need(meta.has("source"), "requires source:")
            need(!meta.has("steps") || meta["steps"]!.isList, "steps: must be a list")
        case "script":
            need(pos.first != nil, "requires a name")
            need(b.args.flags["lang"] != nil, "requires lang=")
            need((b.code ?? "").trimmingCharacters(in: .whitespacesAndNewlines) != "", "requires a code payload after ---")
        case "scenario":
            need(pos.first != nil, "requires a name")
            need(meta["cells"]?.isMap == true, "requires cells:")
            for entry in (meta["cells"]?.mapEntries ?? []) where Refs.parseCell(entry.key.replacingOccurrences(of: "$", with: "")) == nil {
                err(b.line, "scenario cells key must be a cell: \(entry.key)")
            }
        case "raw":
            need(["ooxml", "json", "text"].contains(pos.first ?? ""), "format must be ooxml | json | text")
            if let part = b.args.flags["part"] { need(isValidPartPath(part), "part= fails package-path canonicalization: \(part)") }
            if let enc = b.args.flags["encoding"] { need(enc == "base64", "encoding must be base64") }
        default:
            break
        }
    }

    // ---- workbook-level blocks ----
    for block in doc.workbookBlocks {
        doc.statsBlocks += 1
        switch block {
        case .at(let a):
            err(a.line, "@ directives are not allowed before the first sheet")
        case .fence(let b):
            if b.kind.hasPrefix("x-") { continue }
            if !reservedKinds.contains(b.kind) { err(b.line, "unknown directive {\(b.kind)}"); continue }
            if !workbookKinds.contains(b.kind) { err(b.line, "{\(b.kind)} is sheet-scoped and cannot appear before the first sheet"); continue }
            validateFence(b, nil)
        }
    }

    // ---- per-sheet validation ----
    func validateSheet(_ sheet: SheetNode) {
        var defs: [String: Int] = [:]
        var spills: [(c1: Int, r1: Int, c2: Int, r2: Int, line: Int)] = []
        var spillCaches: [Fence] = []
        var sheetMetas: [Fence] = []
        var chartsAtSheet = 0
        var gridContent = 0

        func addDef(_ col: Int, _ row: Int, _ line: Int, _ what: String) {
            if col > Refs.maxCol || row > Refs.maxRow { err(line, "\(what): cell out of bounds"); return }
            let k = Refs.refKey(col, row)
            if let prev = defs[k] { err(line, "\(what): cell defined more than once (previous definition at line \(prev))"); return }
            defs[k] = line
            doc.statsDefs += 1
        }

        func target(_ text: String?, _ line: Int, _ kinds: [String], _ what: String) -> Refs.Target? {
            guard let t = Refs.parseTarget(text ?? ""), kinds.contains(t.kind) else {
                err(line, "\(what): invalid target \(text ?? "")")
                return nil
            }
            if let s = t.sheet, s.lowercased() != sheet.name.lowercased() {
                err(line, "\(what): anchor qualifier \(s)! must name the containing sheet (\(sheet.name))")
            }
            return t
        }

        let ctx = SheetCtx(target: target, addDef: addDef)

        func validateAt(_ b: AtDirective) {
            guard let t = Refs.parseTarget(b.targetText) else { err(b.line, "invalid @ target: \(b.targetText)"); return }
            if let s = t.sheet, s.lowercased() != sheet.name.lowercased() {
                err(b.line, "@ target qualifier \(s)! must name the containing sheet")
            }
            let body = b.body ?? .map([])
            var props: [(String, YamlValue)] = []
            for e in (b.props?.mapEntries ?? []) { props.append((e.key, e.value)) }
            for e in body.mapEntries {
                if let idx = props.firstIndex(where: { $0.0 == e.key }) { props[idx] = (e.key, e.value) } else { props.append((e.key, e.value)) }
            }

            let bodyContentKeys = contentKeys.filter { body.has($0) }
            var scalar: Scalar?
            if let st = b.scalarText {
                let s = ScalarGrammar.parseScalar(st)
                scalar = s
                if let p = s.problem { err(b.line, "scalar: \(p)") }
                if s.cached?.kind == "invalid" { err(b.line, "scalar: \(s.cached?.problem ?? "")") }
                let cachedOnly = bodyContentKeys.count == 1 && bodyContentKeys[0] == "value" && s.kind == "formula"
                if !bodyContentKeys.isEmpty && !cachedOnly { err(b.line, "inline content and body content keys on the same @ directive") }
            }
            let hasFormula = scalar?.kind == "formula" || body.has("formula")
            let hasContent = (scalar != nil && scalar!.kind != "blank") || !bodyContentKeys.isEmpty

            if hasContent {
                if t.kind == "cell", let box = t.box {
                    addDef(box.c1, box.r1, b.line, "@")
                } else if t.kind == "range", hasFormula, let box = t.box {
                    let count = (box.r2 - box.r1 + 1) * (box.c2 - box.c1 + 1)
                    if count > fillEnumerationCap {
                        warn(b.line, "relative fill over \(count) cells — overlap checking skipped")
                    } else {
                        for r in box.r1...box.r2 { for c in box.c1...box.c2 { addDef(c, r, b.line, "@ fill") } }
                    }
                } else {
                    err(b.line, "range targets accept formula content only (relative fill, SPEC §8.5/§9.4)")
                }
            }

            for (k, v) in props {
                if !knownProps.contains(k) && !k.hasPrefix("x-") { warn(b.line, "unknown property: \(k)") }
                if (k == "fill" || k == "color") && !isColorValue(v) { err(b.line, "\(k): not a color: \(v.jsString)") }
                if k == "link" && !isSafeLink(v) { err(b.line, "link: scheme must be https:, mailto:, or internal #: \(v.jsString)") }
                if k == "merge" {
                    if t.kind != "range" { err(b.line, "merge: requires a range target") }
                    if v != .bool(true) { err(b.line, "merge: only `true` is valid") }
                }
                if k == "spill" || k == "array" {
                    guard let st = Refs.parseTarget(v.jsString), st.kind == "range", let sbox = st.box else { err(b.line, "\(k): must be a range"); continue }
                    if t.kind != "cell" || (t.box.map { sbox.c1 != $0.c1 || sbox.r1 != $0.r1 } ?? true) {
                        err(b.line, "\(k): range must start at the anchor cell")
                    }
                    spills.append((sbox.c1, sbox.r1, sbox.c2, sbox.r2, b.line))
                }
                if k == "rich" && !v.isList { err(b.line, "rich: must be a list of runs") }
                if k == "control" && v != .string("checkbox") { err(b.line, "control: unknown control \(v.jsString)") }
            }
            if body.has("formula") && !body.has("value") { warn(b.line, "formula without a cached value: readers will need a calc engine to display") }
        }

        func validateSheetMeta(_ b: Fence) {
            let m = b.meta ?? .map([])
            for entry in m.mapEntries where !sheetMetaKeys.contains(entry.key) && !entry.key.hasPrefix("x-") {
                warn(b.line, "unknown {sheet} key: \(entry.key)")
            }
            if let kind = m["kind"], !["worksheet", "chart"].contains(kind.asString ?? "") { err(b.line, "{sheet} kind must be worksheet | chart") }
            if let tc = m["tab-color"], !isColorValue(tc) { err(b.line, "tab-color: not a color: \(tc.jsString)") }
            if let h = m["hidden"], !(h == .bool(true) || h == .bool(false) || h == .string("very")) { err(b.line, "hidden must be false | true | very") }
            for key in ["freeze", "split"] {
                if let v = m[key], Refs.parseCell(v.jsString) == nil { err(b.line, "\(key): must be a cell reference") }
            }
            for entry in (m["cols"]?.mapEntries ?? []) {
                if !matches(colRangeKeyRE, entry.key) { err(b.line, "cols key must be a column or column range: \(entry.key)") }
                let v = entry.value
                let isNumber = v.asInt != nil || v.asDouble != nil
                let isObject = v.isMap || v.isList
                if !isNumber && !isObject { err(b.line, "cols.\(entry.key): must be a width or a mapping") }
            }
            for entry in (m["rows"]?.mapEntries ?? []) where !matches(rowKeyRE, entry.key) {
                err(b.line, "rows key must be a row or row range: \(entry.key)")
            }
        }

        for block in sheet.blocks {
            doc.statsBlocks += 1
            switch block {
            case .at(let a):
                validateAt(a)
            case .fence(let b):
                if b.kind.hasPrefix("x-") { continue }
                if !reservedKinds.contains(b.kind) { err(b.line, "unknown directive {\(b.kind)}"); continue }
                if b.kind == "sheet" { sheetMetas.append(b); validateSheetMeta(b); continue }
                if b.kind == "grid" || b.kind == "table" { gridContent += 1 }
                if b.kind == "spill-cache" { spillCaches.append(b); continue }
                if b.kind == "chart", b.args.anchor == "sheet" { chartsAtSheet += 1 }
                validateFence(b, ctx)
            }
        }

        if sheetMetas.count > 1 { err(sheetMetas[1].line, "multiple {sheet} blocks in one sheet") }
        if let first = sheetMetas.first, !(sheet.blocks.first.map { isSameFence($0, first) } ?? false) {
            warn(first.line, "{sheet} should be the first block of its sheet")
        }
        let meta = sheetMetas.first?.meta ?? .map([])

        if meta["kind"]?.asString == "chart" {
            if chartsAtSheet != 1 { err(sheet.line, "a chart sheet requires exactly one {chart} anchored `at sheet` (found \(chartsAtSheet))") }
            if gridContent > 0 || !defs.isEmpty { err(sheet.line, "a chart sheet cannot carry worksheet grid content") }
        } else if chartsAtSheet > 0 {
            err(sheet.line, "`at sheet` chart anchors require {sheet} kind: chart")
        }

        for sc in spillCaches {
            guard let anchor = Refs.parseCell(sc.args.positional.first ?? "") else { err(sc.line, "{spill-cache} requires a cell anchor"); continue }
            let rows = sc.rows ?? []
            let h = rows.count
            let w = rows.map { $0.cells.count }.max() ?? 0
            guard let owner = spills.first(where: { $0.c1 == anchor.col && $0.r1 == anchor.row }) else {
                err(sc.line, "{spill-cache} at \(sc.args.positional.first ?? "") has no owning spill/array formula at that anchor")
                continue
            }
            if anchor.row + h - 1 > owner.r2 || anchor.col + w - 1 > owner.c2 {
                err(sc.line, "{spill-cache} rectangle exceeds the declared spill/array range")
            }
        }
    }

    if doc.sheets.isEmpty { err(1, "a workbook requires at least one sheet (a level-1 heading)") }
    var sheetNames: [String: Bool] = [:]
    for sheet in doc.sheets {
        let nameKey = sheet.name.lowercased()
        if sheet.name.count > 31 { err(sheet.line, "sheet name exceeds 31 chars: \(sheet.name)") }
        if matches(sheetNameBadRE, sheet.name) { err(sheet.line, "sheet name contains a forbidden character (: \\ / ? * [ ]): \(sheet.name)") }
        if sheetNames[nameKey] != nil { err(sheet.line, "duplicate sheet name: \(sheet.name)") }
        sheetNames[nameKey] = true
        validateSheet(sheet)
    }
}

private func isSameFence(_ block: Block, _ fence: Fence) -> Bool {
    if case let .fence(f) = block { return f === fence }
    return false
}
