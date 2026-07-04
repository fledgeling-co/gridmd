// GridMD command-line tool — three conformance verbs:
//
//   gridmd dump <file.gmd>                 canonical model dump to stdout
//   gridmd to-xlsx <file.gmd> -o out.xlsx  export (loud fidelity report)
//   gridmd from-xlsx <file.xlsx> -o out.gmd import (self-checks strict lint)
//
// Dependency-free argv parsing (no swift-argument-parser).

import Foundation
import GridMD

func stderr(_ s: String) {
    FileHandle.standardError.write(Data((s + "\n").utf8))
}

func stdoutRaw(_ s: String) {
    FileHandle.standardOutput.write(Data(s.utf8))
}

func parseIO(_ args: [String]) -> (input: String?, output: String?) {
    var output: String?
    var files: [String] = []
    var i = 0
    while i < args.count {
        if args[i] == "-o", i + 1 < args.count {
            output = args[i + 1]
            i += 2
            continue
        }
        if !args[i].hasPrefix("-") { files.append(args[i]) }
        i += 1
    }
    return (files.first { $0 != output }, output)
}

func readSource(_ path: String) -> String? {
    guard let data = FileManager.default.contents(atPath: path) else { return nil }
    return String(decoding: data, as: UTF8.self)
}

func runDump(_ args: [String]) -> Int32 {
    guard let input = parseIO(args).input else {
        stderr("usage: gridmd dump <file.gmd>")
        return 2
    }
    guard let source = readSource(input) else {
        stderr("\(input): cannot read file")
        return 2
    }
    do {
        stdoutRaw(try GridMD.dump(source))
        return 0
    } catch let GridMD.Failure.invalid(diags) {
        for d in diags { stderr("\(input):\(d.line): error: \(d.message)") }
        return 1
    } catch {
        stderr("\(input): \(error)")
        return 1
    }
}

func replacingExtension(_ path: String, with ext: String) -> String {
    if let dot = path.lastIndex(of: "."), !path[path.index(after: dot)...].contains("/") {
        return String(path[..<dot]) + ext
    }
    return path + ext
}

func runToXlsx(_ args: [String]) -> Int32 {
    let io = parseIO(args)
    guard let input = io.input else {
        stderr("usage: gridmd to-xlsx <file.gmd> -o out.xlsx")
        return 2
    }
    guard let source = readSource(input) else {
        stderr("\(input): cannot read file")
        return 2
    }
    do {
        let res = try GridMD.exportXLSX(source)
        let out = io.output ?? replacingExtension(input, with: ".xlsx")
        try res.data.write(to: URL(fileURLWithPath: out))
        for r in res.report {
            print("\(input): \(r.action): \(r.feature)\(r.note.map { " (\($0))" } ?? "")")
        }
        print("\(out): written (\(res.data.count) bytes) — worksheet core native, remainder carried losslessly")
        return 0
    } catch let GridMD.Failure.invalid(diags) {
        for d in diags { stderr("\(input):\(d.line): error: \(d.message)") }
        stderr("\(input): \(diags.count) error(s) — fix the document before converting")
        return 1
    } catch {
        stderr("\(input): \(error)")
        return 1
    }
}

func runFromXlsx(_ args: [String]) -> Int32 {
    let io = parseIO(args)
    guard let input = io.input else {
        stderr("usage: gridmd from-xlsx <file.xlsx> -o out.gmd")
        return 2
    }
    guard let data = FileManager.default.contents(atPath: input) else {
        stderr("\(input): cannot read file")
        return 2
    }
    do {
        let res = try GridMD.importXLSX(data)
        for r in res.report {
            print("\(input): \(r.action): \(r.feature)\(r.note.map { " (\($0))" } ?? "")")
        }
        let lint = GridMD.lint(res.gmd, strict: true)
        for e in lint.errors { stderr("self-check:\(e.line): error: \(e.message)") }
        let out = io.output ?? replacingExtension(input, with: ".gmd")
        try Data(res.gmd.utf8).write(to: URL(fileURLWithPath: out))
        print("\(out): written — \(lint.sheets) sheet(s), \(lint.definedCells) defined cell(s); self-check \(lint.errors.isEmpty ? "clean" : "FAILED (\(lint.errors.count) error(s))")")
        return lint.errors.isEmpty ? 0 : 1
    } catch {
        stderr("\(input): \(error)")
        return 1
    }
}

let argv = Array(CommandLine.arguments.dropFirst())
guard let verb = argv.first else {
    stderr("usage: gridmd <dump|to-xlsx|from-xlsx> <file> [-o out]")
    exit(2)
}
let rest = Array(argv.dropFirst())
switch verb {
case "dump": exit(runDump(rest))
case "to-xlsx": exit(runToXlsx(rest))
case "from-xlsx": exit(runFromXlsx(rest))
default:
    stderr("unknown command: \(verb)\nusage: gridmd <dump|to-xlsx|from-xlsx> <file> [-o out]")
    exit(2)
}
