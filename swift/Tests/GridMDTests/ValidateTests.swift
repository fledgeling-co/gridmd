import XCTest
@testable import GridMD

final class ValidateTests: XCTestCase {
    func errs(_ src: String) -> [String] { GridMD.lint(src).errors.map(\.message) }
    func warns(_ src: String) -> [String] { GridMD.lint(src).warnings.map(\.message) }
    func hasErr(_ src: String, _ needle: String) { XCTAssertTrue(errs(src).contains { $0.contains(needle) }, "expected error \"\(needle)\" in \(errs(src))") }
    func hasWarn(_ src: String, _ needle: String) { XCTAssertTrue(warns(src).contains { $0.contains(needle) }, "expected warning \"\(needle)\"") }
    func clean(_ src: String) { XCTAssertTrue(errs(src).isEmpty, "unexpected errors: \(errs(src))") }

    func wrap(_ body: String, fm: String = "gridmd: \"1.0\"") -> String { "---\n\(fm)\n---\n# S\n\(body)" }

    func testFrontmatter() {
        hasErr("---\ngridmd: 1\n---\n# S\n@ A1 1", "gridmd:")
        hasWarn("---\ngridmd: \"1.0\"\nweird: 1\n---\n# S\n@ A1 1", "unknown frontmatter key")
        hasErr("---\ngridmd: \"1.0\"\ndate-system: 1999\n---\n# S\n@ A1 1", "date-system must be")
        hasErr("---\ngridmd: \"1.0\"\ncalc: { mode: nope }\n---\n# S\n@ A1 1", "calc.mode must be")
        hasErr("---\ngridmd: \"1.0\"\nnames:\n  - { ref: A1 }\n---\n# S\n@ A1 1", "names entries require a name")
        hasErr("---\ngridmd: \"1.0\"\nnames:\n  - { name: X, ref: A1, formula: B1 }\n---\n# S\n@ A1 1", "exactly one of")
        hasErr("---\ngridmd: \"1.0\"\nnames:\n  - { name: X, ref: A1 }\n  - { name: x, ref: B1 }\n---\n# S\n@ A1 1", "duplicate defined name")
        hasErr("---\ngridmd: \"1.0\"\nstyles:\n  bad: notamap\n---\n# S\n@ A1 1", "must be a mapping")
        hasWarn("---\ngridmd: \"1.0\"\ntheme: { colors: { zzz: \"#FFFFFF\" } }\n---\n# S\n@ A1 1", "unknown theme color slot")
        hasErr("---\ngridmd: \"1.0\"\ntheme: { colors: { accent1: nothex } }\n---\n# S\n@ A1 1", "theme color")
    }

    func testWorkbookScope() {
        hasErr("---\ngridmd: \"1.0\"\n---\n@ A1 1\n# S\n@ A1 2", "not allowed before the first sheet")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{bogus}\n```\n# S\n@ A1 1", "unknown directive")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{cf} A1\n```\n# S\n@ A1 1", "sheet-scoped and cannot appear")
        hasErr("---\ngridmd: \"1.0\"\n---\n", "requires at least one sheet")
    }

    func testSheetNames() {
        hasErr(wrap("@ A1 1", fm: "gridmd: \"1.0\"").replacingOccurrences(of: "# S", with: "# " + String(repeating: "x", count: 32)), "exceeds 31")
        hasErr("---\ngridmd: \"1.0\"\n---\n# a/b\n@ A1 1", "forbidden character")
        hasErr("---\ngridmd: \"1.0\"\n---\n# Dup\n@ A1 1\n# dup\n@ A1 2", "duplicate sheet name")
    }

    func testGridAndTable() {
        hasErr(wrap("```{grid} ZZ\n| 1 |\n```"), "requires a cell anchor")
        hasErr(wrap("```{grid} A1\n| \"open |\n```"), "grid cell:")
        hasErr(wrap("```{table} 1A at A1\n---\n| a |\n```"), "valid table name")
        hasErr(wrap("```{table} T at ZZ\n---\n| a |\n```"), "requires `at <cell>`")
        hasErr(wrap("```{table} T at A1\n```"), "requires payload rows")
        hasErr(wrap("```{table} T at A1\n---\n| a | a |\n| 1 | 2 |\n```"), "duplicate table column name")
        hasErr(wrap("```{table} T at A1\ncols: { nope: { numfmt: \"0\" } }\n---\n| a |\n| 1 |\n```"), "cols references unknown column")
        hasErr(wrap("```{table} T at A1\nsort:\n  - { col: nope }\n---\n| a |\n| 1 |\n```"), "sort references unknown column")
        hasErr(wrap("```{table} T at A1\n---\n| 5 | a |\n| 1 | 2 |\n```"), "header cells must be non-empty text")
        // header:false path + no-total
        clean(wrap("```{table} T at A1\nheader: false\n---\n| 1 | 2 |\n```"))
    }

    func testTableNameCollision() {
        hasErr(wrap("```{table} T at A1\n---\n| a |\n| 1 |\n```\n```{table} T at C1\n---\n| b |\n| 2 |\n```"), "collides with an existing name")
    }

    func testCf() {
        hasErr(wrap("```{cf} zz\n- when: \"> 1\"\n```"), "invalid target")
        hasErr(wrap("```{cf} A1\nnotalist: 1\n```"), "must be a YAML list")
        hasErr(wrap("```{cf} A1\n- {}\n```"), "exactly one distinguishing key")
        hasErr(wrap("```{cf} A1\n- when: x\n  priority: 0\n```"), "positive integer")
        hasErr(wrap("```{cf} A1\n- when: x\n  format: { fill: notcolor }\n```"), "not a color")
    }

    func testValidation() {
        hasErr(wrap("```{validation} A1\ntype: bogus\n```"), "type must be one of")
        hasErr(wrap("```{validation} A1\ntype: list\n```"), "requires values: or source:")
        hasErr(wrap("```{validation} A1\ntype: whole\nerror: { style: bad }\n```"), "error.style must be")
    }

    func testFilter() {
        hasErr(wrap("```{filter} A1:C3\ncols: { zz: {} }\n```"), "column letters")
    }

    func testChart() {
        hasWarn(wrap("```{chart} bogustype at A1\nseries:\n  - { val: A1 }\n```"), "unknown chart type")
        hasErr(wrap("```{chart} column\nseries:\n  - { val: A1 }\n```"), "requires `at <anchor>`")
        hasErr(wrap("```{chart} column at A1\n```"), "requires series:")
        hasErr(wrap("```{chart} column at A1\nseries:\n  - { name: x }\n```"), "requires val:")
        hasErr(wrap("```{chart} column at A1\nseries:\n  - { val: A1, color: notcolor }\n```"), "not a color")
    }

    func testObjects() {
        hasErr(wrap("```{sparklines} A1\n```"), "requires source:")
        hasErr(wrap("```{sparklines} A1\nsource: B1:B3\ntype: bad\n```"), "type must be")
        hasErr(wrap("```{pivot} P at ZZ\nsource: X\n```"), "requires `at <cell>`")
        hasErr(wrap("```{pivot} P at A1\n```"), "requires source:")
        hasErr(wrap("```{slicer} at A1\n```"), "requires for: and field:")
        hasErr(wrap("```{image} at A1\n```"), "requires src:")
        hasErr(wrap("```{image} at A1\nsrc: \"javascript:alert(1)\"\n```"), "scheme allowlist")
        hasWarn(wrap("```{shape} weird at A1\n```"), "unknown shape kind")
        hasErr(wrap("```{shape} rect\n```"), "requires an anchor")
        hasErr(wrap("```{textbox}\ntext: hi\n```"), "requires an anchor")
        hasErr(wrap("```{checkbox}\n```"), "requires an anchor")
        hasErr(wrap("```{checkbox} at A1\nlinked: nope\n```"), "linked: must be a cell")
        hasErr(wrap("```{comments} zz\n- { by: a, at: b, text: c }\n```"), "invalid target")
        hasErr(wrap("```{comments} A1\nnotalist: 1\n```"), "must be a YAML list of comments")
        hasErr(wrap("```{comments} A1\n- { by: a }\n```"), "requires by:, at:, text:")
        hasErr(wrap("```{outline}\nrows:\n  - { range: nope }\n```"), "outline rows range")
        hasErr(wrap("```{outline}\ncols:\n  - { range: nope }\n```"), "outline cols range")
        hasErr(wrap("```{page}\norientation: sideways\n```"), "orientation must be")
        hasErr(wrap("```{page}\nscale: 90\nfit: { width: 1 }\n```"), "mutually exclusive")
        hasErr(wrap("```{scenario} Sc\n```"), "requires cells:")
        hasErr(wrap("```{scenario} Sc\ncells: { zz: 1 }\n```"), "must be a cell")
    }

    func testWorkbookQueryScriptRaw() {
        hasErr("---\ngridmd: \"1.0\"\n---\n```{query} Q\n```\n# S\n@ A1 1", "requires source:")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{query} Q\nsource: x\nsteps: notalist\n```\n# S\n@ A1 1", "steps: must be a list")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{script} Sc\non: manual\n---\ncode\n```\n# S\n@ A1 1", "requires lang=")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{script} Sc lang=js\n```\n# S\n@ A1 1", "requires a code payload")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{raw} weird\n```\n# S\n@ A1 1", "format must be")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{raw} ooxml part=\"../evil\"\nx\n```\n# S\n@ A1 1", "package-path canonicalization")
        hasErr("---\ngridmd: \"1.0\"\n---\n```{raw} ooxml encoding=hex\nx\n```\n# S\n@ A1 1", "encoding must be base64")
    }

    func testAtDirective() {
        hasErr(wrap("@ ZZ 1"), "invalid @ target")
        hasErr(wrap("@ Other!A1 1"), "must name the containing sheet")
        hasErr(wrap("@ A1 5\n  value: 6"), "inline content and body content keys")
        hasErr(wrap("@ A1:B2 5"), "range targets accept formula content only")
        hasWarn(wrap("@ A1 5 { weirdprop: 1 }"), "unknown property")
        hasErr(wrap("@ A1 5 { fill: notcolor }"), "not a color")
        hasErr(wrap("@ A1 5 { link: \"ftp://x\" }"), "scheme must be")
        hasErr(wrap("@ A1 5 { merge: true }"), "merge: requires a range target")
        hasErr(wrap("@ A1:B2 { merge: yes }"), "only `true` is valid")
        hasErr(wrap("@ A1 =X { spill: notarange }"), "must be a range")
        hasErr(wrap("@ A1 =X { spill: B2:C3 }"), "must start at the anchor cell")
        hasErr(wrap("@ A1 5 { rich: notalist }"), "rich: must be a list")
        hasErr(wrap("@ A1 5 { control: radio }"), "unknown control")
        hasWarn(wrap("@ A1\n  formula: =SUM(B:B)"), "formula without a cached value")
        hasErr(wrap("@ A1 =A1 :: =B1"), "cached value must not be a formula")
    }

    func testSheetMeta() {
        hasErr(wrap("```{sheet}\nkind: bogus\n```"), "kind must be worksheet")
        hasErr(wrap("```{sheet}\ntab-color: nope\n```"), "tab-color: not a color")
        hasErr(wrap("```{sheet}\nhidden: maybe\n```"), "hidden must be")
        hasErr(wrap("```{sheet}\nfreeze: nope\n```"), "must be a cell reference")
        hasErr(wrap("```{sheet}\ncols: { 1: 10 }\n```"), "cols key must be")
        hasErr(wrap("```{sheet}\ncols: { A: notwidth }\n```"), "must be a width or a mapping")
        hasErr(wrap("```{sheet}\nrows: { A: 10 }\n```"), "rows key must be")
        hasWarn(wrap("```{sheet}\nweird: 1\n```"), "unknown {sheet} key")
        hasErr(wrap("```{sheet}\n```\n```{sheet}\n```"), "multiple {sheet} blocks")
        hasWarn(wrap("@ A1 1\n```{sheet}\n```"), "should be the first block")
    }

    func testChartSheetAndSpillCache() {
        hasErr(wrap("```{sheet}\nkind: chart\n```"), "requires exactly one {chart}")
        hasErr(wrap("```{sheet}\nkind: chart\n```\n```{chart} column at sheet\nseries:\n  - { val: A1 }\n```\n@ A1 1"), "cannot carry worksheet grid content")
        hasErr(wrap("```{chart} column at sheet\nseries:\n  - { val: A1 }\n```"), "require {sheet} kind: chart")
        hasErr(wrap("```{spill-cache} ZZ\n| 1 |\n```"), "requires a cell anchor")
        hasErr(wrap("```{spill-cache} D2\n| 1 |\n```"), "no owning spill")
        hasErr(wrap("@ D2 =SORT(A:A) { spill: D2:D3 }\n```{spill-cache} D2\n| 1 |\n| 2 |\n| 3 |\n```"), "exceeds the declared spill")
        clean(wrap("@ D2 =SORT(A:A) { spill: D2:D4 }\n```{spill-cache} D2\n| 1 |\n| 2 |\n| 3 |\n```"))
    }

    func testRelativeFillDefines() {
        clean(wrap("@ B2:B4 =A2*2"))
        // overlapping fill triggers duplicate-definition
        hasErr(wrap("@ B2:B4 =A2*2\n@ B3 9"), "defined more than once")
    }
}
