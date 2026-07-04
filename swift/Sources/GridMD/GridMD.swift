// Public API for the GridMD Swift library.
//
//   let json = try GridMD.dump(source)                 // canonical model dump
//   let xlsx = try GridMD.exportXLSX(source)            // GridMD → .xlsx
//   let back = try GridMD.importXLSX(xlsx.data)         // .xlsx → GridMD
//   let lint = GridMD.lint(source)                      // errors + warnings
//
// The library is pure Swift + Foundation (+ the system Compression framework);
// no third-party dependencies.

import Foundation

public enum GridMD {
    public struct Diagnostic: Equatable, Sendable {
        public let line: Int
        public let message: String
    }

    public struct LintResult: Sendable {
        public let errors: [Diagnostic]
        public let warnings: [Diagnostic]
        public let sheets: Int
        public let definedCells: Int
        public let blocks: Int
        public var isValid: Bool { errors.isEmpty }
    }

    public struct FidelityLine: Sendable {
        public let line: Int
        public let action: String
        public let feature: String
        public let note: String?
    }

    public struct ExportResult: Sendable {
        public let data: Data
        public let report: [FidelityLine]
    }

    public struct ImportResult: Sendable {
        public let gmd: String
        public let report: [FidelityLine]
    }

    public enum Failure: Error, CustomStringConvertible {
        case invalid([Diagnostic])
        case badXLSX(String)

        public var description: String {
            switch self {
            case let .invalid(diags): return "invalid GridMD: \(diags.count) error(s)"
            case let .badXLSX(msg): return "bad .xlsx: \(msg)"
            }
        }
    }

    /// Lints `source` (strict by default). Never throws — inspect `errors`.
    public static func lint(_ source: String, strict: Bool = true) -> LintResult {
        let l = runLint(source, mode: strict ? "strict" : "lenient")
        return LintResult(
            errors: l.errors.map { Diagnostic(line: $0.line, message: $0.msg) },
            warnings: l.warnings.map { Diagnostic(line: $0.line, message: $0.msg) },
            sheets: l.doc.sheets.count,
            definedCells: l.doc.statsDefs,
            blocks: l.doc.statsBlocks
        )
    }

    /// Canonical conformance dump. Throws `Failure.invalid` if strict lint fails.
    public static func dump(_ source: String) throws -> String {
        let l = runLint(source, mode: "strict")
        if !l.errors.isEmpty { throw Failure.invalid(l.errors.map { Diagnostic(line: $0.line, message: $0.msg) }) }
        return Dump.model(Model.build(l.doc))
    }

    /// Exports GridMD → `.xlsx` (worksheet core native + full-source carry part).
    /// Throws `Failure.invalid` if strict lint fails.
    public static func exportXLSX(_ source: String) throws -> ExportResult {
        let l = runLint(source, mode: "strict")
        if !l.errors.isEmpty { throw Failure.invalid(l.errors.map { Diagnostic(line: $0.line, message: $0.msg) }) }
        let (bytes, report) = Xlsx.write(model: Model.build(l.doc), source: source)
        return ExportResult(data: Data(bytes), report: report.map(mapReport))
    }

    /// Imports `.xlsx` → GridMD (restored losslessly from the carry part).
    public static func importXLSX(_ data: Data) throws -> ImportResult {
        do {
            let (gmd, report) = try Xlsx.read(Array(data))
            return ImportResult(gmd: gmd, report: report.map(mapReport))
        } catch {
            throw Failure.badXLSX(String(describing: error))
        }
    }

    private static func mapReport(_ r: ReportLine) -> FidelityLine {
        FidelityLine(line: r.line, action: r.action, feature: r.feature, note: r.note)
    }
}

struct InternalLint {
    let doc: Document
    let errors: [Diagnostic]
    let warnings: [Diagnostic]
}

/// Parses + validates, returning diagnostics sorted stably by line (matching the
/// JS reference's `[...].sort((a,b) => a.line - b.line)`).
func runLint(_ source: String, mode: String) -> InternalLint {
    let doc = Parser.parseDocument(source, mode: mode)
    validateDocument(doc)
    return InternalLint(doc: doc, errors: stableSortByLine(doc.errors), warnings: stableSortByLine(doc.warnings))
}

private func stableSortByLine(_ diags: [Diagnostic]) -> [Diagnostic] {
    diags.enumerated()
        .sorted { a, b in a.element.line != b.element.line ? a.element.line < b.element.line : a.offset < b.offset }
        .map { $0.element }
}
