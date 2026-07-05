import XCTest
@testable import GridMD

final class ParserTests: XCTestCase {
    func testFindPropsSplit() {
        let (s1, p1) = Parser.findPropsSplit("=SUM(A1:A3) :: 3 { style: money }")
        XCTAssertEqual(s1, "=SUM(A1:A3) :: 3")
        XCTAssertEqual(p1, "{ style: money }")
        XCTAssertNil(Parser.findPropsSplit("no braces").propsText)
        XCTAssertNil(Parser.findPropsSplit("{ leading } trailing").propsText) // not at end
        XCTAssertNil(Parser.findPropsSplit("value}").propsText) // unbalanced (no space before)
        XCTAssertNil(Parser.findPropsSplit("}").propsText) // depth < 0
        // quotes are respected
        let (s2, p2) = Parser.findPropsSplit("\"a } b\" { x: 1 }")
        XCTAssertEqual(s2, "\"a } b\"")
        XCTAssertEqual(p2, "{ x: 1 }")
    }

    func testSplitPipeRow() {
        XCTAssertEqual(Parser.splitPipeRow("| a | b |"), ["a", "b"])
        XCTAssertEqual(Parser.splitPipeRow("| a \\| b | c |"), ["a | b", "c"])
        XCTAssertNil(Parser.splitPipeRow("no pipe"))
        XCTAssertNil(Parser.splitPipeRow("| unterminated"))
        XCTAssertNil(Parser.splitPipeRow("|"))
    }

    func testParseInfoArgs() {
        var errs: [Diagnostic] = []
        let a = Parser.parseInfoArgs("column \"Qty by item\" at H6:M18 lang=js size 480x320", 1, &errs)
        XCTAssertEqual(a.positional, ["column", "Qty by item"])
        XCTAssertEqual(a.anchor, "H6:M18")
        XCTAssertEqual(a.flags["lang"], "js")
        XCTAssertEqual(a.size?.w, 480)
        XCTAssertEqual(a.size?.h, 320)
        XCTAssertTrue(errs.isEmpty)
    }

    func testParseInfoArgsErrors() {
        var errs: [Diagnostic] = []
        _ = Parser.parseInfoArgs("at", 1, &errs)
        _ = Parser.parseInfoArgs("size nope", 1, &errs)
        XCTAssertEqual(errs.count, 2)
    }

    func testFlagQuoteStripping() {
        var errs: [Diagnostic] = []
        let a = Parser.parseInfoArgs("part=\"customXml/item1.xml\"", 1, &errs)
        XCTAssertEqual(a.flags["part"], "customXml/item1.xml")
    }

    func testFrontmatterErrors() {
        XCTAssertFalse(Parser.parseDocument("no frontmatter").errors.isEmpty)
        XCTAssertFalse(Parser.parseDocument("---\ngridmd: \"1.0\"\n").errors.isEmpty) // unterminated
    }

    func testDocStructure() {
        let src = """
        ---
        gridmd: "1.0"
        ---

        > a doc comment
        ## a subheading

        # Sheet1
        @ A1 "hi"
        ```{grid} B2
        | 1 | 2 |
        ```
        """
        let doc = Parser.parseDocument(src)
        XCTAssertEqual(doc.sheets.count, 1)
        XCTAssertEqual(doc.sheets[0].name, "Sheet1")
        XCTAssertEqual(doc.sheets[0].blocks.count, 2)
    }

    func testMultilineAtBody() {
        let src = """
        ---
        gridmd: "1.0"
        ---
        # S
        @ B4
          note: |
            A cell note.

        @ C4 42
        """
        let doc = Parser.parseDocument(src)
        guard case let .at(a) = doc.sheets[0].blocks[0] else { return XCTFail() }
        XCTAssertEqual(a.body?["note"], .string("A cell note.\n"))
    }

    func testUnclosedFenceAndUnrecognized() {
        let strict = Parser.parseDocument("---\ngridmd: \"1.0\"\n---\n# S\n```{grid} A1\n| 1 |")
        XCTAssertTrue(strict.errors.contains { $0.msg.contains("unclosed") })
        let lenient = Parser.parseDocument("---\ngridmd: \"1.0\"\n---\n# S\nbogus line", mode: "lenient")
        XCTAssertTrue(lenient.errors.isEmpty)
        XCTAssertFalse(lenient.warnings.isEmpty)
        let strict2 = Parser.parseDocument("---\ngridmd: \"1.0\"\n---\n# S\nbogus line")
        XCTAssertFalse(strict2.errors.isEmpty)
    }

    func testProsSplitInlineBrace() {
        // @ target { ...props } (inline flow-map props, no scalar)
        let doc = Parser.parseDocument("---\ngridmd: \"1.0\"\n---\n# S\n@ C7 { fill: \"#FDECEC\" }")
        guard case let .at(a) = doc.sheets[0].blocks[0] else { return XCTFail() }
        XCTAssertEqual(a.props?["fill"], .string("#FDECEC"))
        XCTAssertNil(a.scalarText)
    }
}
