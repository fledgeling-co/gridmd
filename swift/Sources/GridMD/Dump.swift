// Canonical model dump — the cross-language conformance contract (js/src/dump.js).
// Byte-identical JSON across every GridMD implementation: fixed key order,
// shortest round-trip numbers, cells row-major, merges/tables/names sorted.

enum Dump {
    private static func scalarDump(_ s: Scalar?) -> JSONValue {
        guard let s else { return .null }
        switch s.kind {
        case "number": return .object([("t", .string("n")), ("v", .double(s.numberValue ?? 0))])
        case "boolean": return .object([("t", .string("b")), ("v", .bool(s.boolValue ?? false))])
        case "error": return .object([("t", .string("e")), ("v", .string(s.stringValue ?? ""))])
        case "date", "time": return .object([("t", .string("d")), ("v", .string(s.stringValue ?? ""))])
        default: return .object([("t", .string("s")), ("v", .string(s.stringValue ?? ""))])
        }
    }

    /// Renders the canonical dump JSON (with trailing newline) for a workbook model.
    static func model(_ model: WorkbookModel) -> String {
        let fm = model.fm

        let gridmd: JSONValue = fm["gridmd"].map { .string($0.jsString) } ?? .null
        let title: JSONValue = fm["title"].map { .string($0.jsString) } ?? .null
        let dateSystem: JSONValue = .int(fm["date-system"]?.asInt == 1904 ? 1904 : 1900)

        var names: [JSONValue] = (fm["names"]?.listItems ?? []).map { n in
            JSONValue.object([
                ("name", .string(n["name"]?.jsString ?? "")),
                ("ref", n["ref"].map { .string($0.jsString) } ?? .null),
                ("formula", n["formula"].map { .string($0.jsString) } ?? .null),
                ("value", n["value"].map { .string($0.jsString) } ?? .null),
            ])
        }
        names.sort { objName($0) < objName($1) }

        let sheets: [JSONValue] = model.sheets.map { s in
            let meta = s.meta
            let hidden: JSONValue = meta["hidden"] == .bool(true)
                ? .bool(true)
                : (meta["hidden"] == .string("very") ? .string("very") : .bool(false))
            let freeze: JSONValue = meta["freeze"].map { .string($0.jsString) } ?? .null
            let isProtected = meta["protect"]?["enabled"]?.isTruthy ?? false

            let sortedCells = s.cellsByKey.values
                .filter { $0.content != nil }
                .sorted { a, b in a.row != b.row ? a.row < b.row : a.col < b.col }
            var cellEntries: [(String, JSONValue)] = []
            for c in sortedCells {
                let ref = "\(Refs.numToCol(c.col))\(c.row)"
                cellEntries.append((ref, dumpCell(c.content!)))
            }

            let merges: [JSONValue] = s.merges
                .map { "\(Refs.numToCol($0.c1))\($0.r1):\(Refs.numToCol($0.c2))\($0.r2)" }
                .sorted()
                .map { .string($0) }

            var tables: [JSONValue] = s.tables.map { t in
                JSONValue.object([
                    ("name", .string(t.name)),
                    ("anchor", .string("\(Refs.numToCol(t.anchor.col))\(t.anchor.row)")),
                    ("columns", .array(t.columns.map { .string($0) })),
                    ("bodyRows", .int(t.bodyRows)),
                    ("hasTotals", .bool(t.hasTotals)),
                ])
            }
            tables.sort { objName($0) < objName($1) }

            let counts = JSONValue.object([
                ("cf", .int(s.cfRuleCounts.reduce(0, +))),
                ("validations", .int(s.validations)),
                ("notes", .int(s.notes)),
                ("threads", .int(s.threads)),
                ("scenarios", .int(s.scenarios)),
                ("sparklines", .int(s.sparklines)),
                ("charts", .int(s.charts)),
                ("pivots", .int(s.pivots)),
                ("slicers", .int(s.slicers)),
                ("images", .int(s.images)),
                ("shapes", .int(s.shapes)),
                ("hyperlinks", .int(s.hyperlinks)),
            ])

            return JSONValue.object([
                ("name", .string(s.name)),
                ("kind", .string(s.kind)),
                ("hidden", hidden),
                ("freeze", freeze),
                ("protected", .bool(isProtected)),
                ("cells", .object(cellEntries)),
                ("merges", .array(merges)),
                ("tables", .array(tables)),
                ("counts", counts),
            ])
        }

        let out = JSONValue.object([
            ("gridmd", gridmd),
            ("title", title),
            ("dateSystem", dateSystem),
            ("names", .array(names)),
            ("sheets", .array(sheets)),
        ])
        return JSON.stringify(out) + "\n"
    }

    private static func dumpCell(_ ct: CellContent) -> JSONValue {
        if let rich = ct.rich {
            let text = rich.map { $0["text"].map { $0.jsString } ?? "" }.joined()
            return .object([("t", .string("rich")), ("v", .string(text))])
        }
        if let f = ct.formula {
            return .object([
                ("t", .string("f")),
                ("f", .string(f)),
                ("cached", scalarDump(ct.cached)),
                ("array", ct.arrayRef.map { .string($0) } ?? .null),
            ])
        }
        return scalarDump(ct.scalar)
    }

    /// Reads the "name" field of a dumped name/table object (for stable sorting).
    private static func objName(_ v: JSONValue) -> String {
        if case let .object(entries) = v, case let .string(s)? = entries.first(where: { $0.0 == "name" })?.1 { return s }
        return ""
    }
}
