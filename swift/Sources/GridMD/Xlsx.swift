// GridMD ⇄ XLSX (Tier-1). The worksheet core (cells + merges) is emitted
// natively so the produced .xlsx is a genuine, openable spreadsheet; the FULL
// original GridMD source is additionally carried, base64-encoded, in a custom
// package part `customXml/gridmdCarry.xml`. Nothing is ever dropped (SPEC §11's
// cardinal rule) — the carry part is the authoritative, lossless round-trip
// path the importer restores from. This is the cheapest correct strategy the
// port runner contract sanctions (carry GridMD definitions in a custom part
// rather than re-authoring chart/pivot/slicer/image/shape OOXML).

import Foundation

struct ReportLine {
    let line: Int
    let action: String
    let feature: String
    let note: String?
}

enum Xlsx {
    static let carryPart = "customXml/gridmdCarry.xml"

    // MARK: - Export (GridMD → XLSX)

    static func write(model: WorkbookModel, source: String) -> (data: [UInt8], report: [ReportLine]) {
        var report: [ReportLine] = []
        var entries: [ZipEntry] = []

        let sheetCount = model.sheets.count
        entries.append(ZipEntry(name: "[Content_Types].xml", data: Array(contentTypes(sheetCount).utf8)))
        entries.append(ZipEntry(name: "_rels/.rels", data: Array(rootRels.utf8)))
        entries.append(ZipEntry(name: "xl/workbook.xml", data: Array(workbookXml(model).utf8)))
        entries.append(ZipEntry(name: "xl/_rels/workbook.xml.rels", data: Array(workbookRels(sheetCount).utf8)))
        entries.append(ZipEntry(name: "xl/styles.xml", data: Array(stylesXml.utf8)))

        for (i, sheet) in model.sheets.enumerated() {
            entries.append(ZipEntry(name: "xl/worksheets/sheet\(i + 1).xml", data: Array(worksheetXml(sheet).utf8)))
            report.append(ReportLine(line: 0, action: "emitted", feature: "worksheet \(sheet.name) core (cells + merges)", note: nil))
            appendCarryReport(sheet, &report)
        }

        let carry = "<gridmdCarry xmlns=\"urn:gridmd:carry\" encoding=\"base64\">" +
            Data(source.utf8).base64EncodedString() +
            "</gridmdCarry>"
        entries.append(ZipEntry(name: carryPart, data: Array(carry.utf8)))
        report.append(ReportLine(line: 0, action: "carried", feature: "GridMD source (\(source.utf8.count) bytes)",
                                 note: "\(carryPart) — full-fidelity lossless round-trip"))

        return (Zip.write(entries), report)
    }

    private static func appendCarryReport(_ s: ModelSheet, _ report: inout [ReportLine]) {
        func note(_ n: Int, _ label: String) {
            if n > 0 { report.append(ReportLine(line: 0, action: "carried", feature: "\(s.name): \(n) \(label)", note: "in \(carryPart)")) }
        }
        note(s.cfRuleCounts.reduce(0, +), "conditional-format rule(s)")
        note(s.validations, "data-validation(s)")
        note(s.charts, "chart(s)")
        note(s.pivots, "pivot(s)")
        note(s.slicers, "slicer(s)")
        note(s.images, "image(s)")
        note(s.shapes, "shape(s)")
        note(s.threads, "threaded comment(s)")
        note(s.scenarios, "scenario(s)")
        note(s.sparklines, "sparkline group(s)")
    }

    // MARK: - Import (XLSX → GridMD)

    enum ImportError: Error, CustomStringConvertible {
        case noCarry
        case badBase64
        var description: String {
            switch self {
            case .noCarry: return "no GridMD carry part (\(carryPart)) found — cannot losslessly restore"
            case .badBase64: return "GridMD carry part is not valid base64"
            }
        }
    }

    static func read(_ data: [UInt8]) throws -> (gmd: String, report: [ReportLine]) {
        let parts = try Zip.read(data)
        guard let carry = parts.first(where: { $0.name == carryPart }) else { throw ImportError.noCarry }
        let xml = String(decoding: carry.data, as: UTF8.self)
        guard let open = xml.firstIndex(of: ">"),
              let close = xml.range(of: "</gridmdCarry>")
        else { throw ImportError.badBase64 }
        let b64 = String(xml[xml.index(after: open)..<close.lowerBound]).trimmingCharacters(in: .whitespacesAndNewlines)
        guard let decoded = Data(base64Encoded: b64), let gmd = String(data: decoded, encoding: .utf8) else {
            throw ImportError.badBase64
        }
        let report = [ReportLine(line: 0, action: "restored", feature: "GridMD document from \(carryPart)", note: "lossless")]
        return (gmd, report)
    }

    // MARK: - XML part builders

    private static func contentTypes(_ sheetCount: Int) -> String {
        var overrides = "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>"
        overrides += "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>"
        for i in 1...max(sheetCount, 1) where sheetCount > 0 {
            overrides += "<Override PartName=\"/xl/worksheets/sheet\(i).xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        }
        return "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>" +
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">" +
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>" +
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>" +
            overrides + "</Types>"
    }

    private static let rootRels =
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>" +
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">" +
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>" +
        "</Relationships>"

    private static func workbookXml(_ model: WorkbookModel) -> String {
        var sheets = ""
        for (i, s) in model.sheets.enumerated() {
            sheets += "<sheet name=\"\(xmlAttr(s.name))\" sheetId=\"\(i + 1)\" r:id=\"rId\(i + 1)\"/>"
        }
        return "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>" +
            "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" " +
            "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">" +
            "<sheets>\(sheets)</sheets></workbook>"
    }

    private static func workbookRels(_ sheetCount: Int) -> String {
        var rels = ""
        for i in 1...max(sheetCount, 1) where sheetCount > 0 {
            rels += "<Relationship Id=\"rId\(i)\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet\(i).xml\"/>"
        }
        rels += "<Relationship Id=\"rId\(sheetCount + 1)\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>"
        return "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>" +
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\(rels)</Relationships>"
    }

    private static let stylesXml =
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>" +
        "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">" +
        "<fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>" +
        "<fills count=\"1\"><fill><patternFill patternType=\"none\"/></fill></fills>" +
        "<borders count=\"1\"><border/></borders>" +
        "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>" +
        "<cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>" +
        "</styleSheet>"

    private static func worksheetXml(_ sheet: ModelSheet) -> String {
        let cells = sheet.cellsByKey.values
            .filter { $0.content != nil }
            .sorted { a, b in a.row != b.row ? a.row < b.row : a.col < b.col }

        var rowsXml = ""
        var idx = 0
        while idx < cells.count {
            let row = cells[idx].row
            var cellsXml = ""
            while idx < cells.count, cells[idx].row == row {
                let c = cells[idx]
                cellsXml += cellXml("\(Refs.numToCol(c.col))\(c.row)", c.content!)
                idx += 1
            }
            rowsXml += "<row r=\"\(row)\">\(cellsXml)</row>"
        }

        var mergeXml = ""
        if !sheet.merges.isEmpty {
            let refs = sheet.merges.map { "\(Refs.numToCol($0.c1))\($0.r1):\(Refs.numToCol($0.c2))\($0.r2)" }.sorted()
            mergeXml = "<mergeCells count=\"\(refs.count)\">" + refs.map { "<mergeCell ref=\"\($0)\"/>" }.joined() + "</mergeCells>"
        }

        return "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>" +
            "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">" +
            "<sheetData>\(rowsXml)</sheetData>\(mergeXml)</worksheet>"
    }

    private static func cellXml(_ ref: String, _ ct: CellContent) -> String {
        if let rich = ct.rich {
            let text = rich.map { $0["text"].map { $0.jsString } ?? "" }.joined()
            return inlineStr(ref, text)
        }
        if let f = ct.formula {
            let fXml = "<f>\(xmlText(f))</f>"
            guard let cached = ct.cached else { return "<c r=\"\(ref)\">\(fXml)</c>" }
            switch cached.kind {
            case "number": return "<c r=\"\(ref)\">\(fXml)<v>\(ESNumber.string(cached.numberValue ?? 0))</v></c>"
            case "boolean": return "<c r=\"\(ref)\" t=\"b\">\(fXml)<v>\(cached.boolValue == true ? 1 : 0)</v></c>"
            case "error": return "<c r=\"\(ref)\" t=\"e\">\(fXml)<v>\(xmlText(cached.stringValue ?? ""))</v></c>"
            default: return "<c r=\"\(ref)\" t=\"str\">\(fXml)<v>\(xmlText(cached.stringValue ?? ""))</v></c>"
            }
        }
        guard let s = ct.scalar else { return "<c r=\"\(ref)\"/>" }
        switch s.kind {
        case "number": return "<c r=\"\(ref)\"><v>\(ESNumber.string(s.numberValue ?? 0))</v></c>"
        case "boolean": return "<c r=\"\(ref)\" t=\"b\"><v>\(s.boolValue == true ? 1 : 0)</v></c>"
        case "error": return "<c r=\"\(ref)\" t=\"e\"><v>\(xmlText(s.stringValue ?? ""))</v></c>"
        default: return inlineStr(ref, s.stringValue ?? "")
        }
    }

    private static func inlineStr(_ ref: String, _ text: String) -> String {
        "<c r=\"\(ref)\" t=\"inlineStr\"><is><t xml:space=\"preserve\">\(xmlText(text))</t></is></c>"
    }

    private static func xmlText(_ s: String) -> String {
        var out = ""
        for ch in s {
            switch ch {
            case "&": out += "&amp;"
            case "<": out += "&lt;"
            case ">": out += "&gt;"
            default: out.append(ch)
            }
        }
        return out
    }

    private static func xmlAttr(_ s: String) -> String {
        var out = ""
        for ch in s {
            switch ch {
            case "&": out += "&amp;"
            case "<": out += "&lt;"
            case ">": out += "&gt;"
            case "\"": out += "&quot;"
            default: out.append(ch)
            }
        }
        return out
    }
}
