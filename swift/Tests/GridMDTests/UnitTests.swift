import XCTest
@testable import GridMD

final class JsonTests: XCTestCase {
    func testScalars() {
        XCTAssertEqual(JSON.stringify(.null), "null")
        XCTAssertEqual(JSON.stringify(.bool(true)), "true")
        XCTAssertEqual(JSON.stringify(.bool(false)), "false")
        XCTAssertEqual(JSON.stringify(.int(1900)), "1900")
        XCTAssertEqual(JSON.stringify(.double(0.3)), "0.3")
        XCTAssertEqual(JSON.stringify(.double(3)), "3")
        XCTAssertEqual(JSON.stringify(.string("hi")), "\"hi\"")
    }

    func testEmptyContainers() {
        XCTAssertEqual(JSON.stringify(.array([])), "[]")
        XCTAssertEqual(JSON.stringify(.object([])), "{}")
    }

    func testIndentAndOrder() {
        let v = JSONValue.object([
            ("a", .int(1)),
            ("b", .array([.int(2), .int(3)])),
        ])
        XCTAssertEqual(JSON.stringify(v), "{\n \"a\": 1,\n \"b\": [\n  2,\n  3\n ]\n}")
    }

    func testEscaping() {
        XCTAssertEqual(JSON.escape("a\"b\\c"), "\"a\\\"b\\\\c\"")
        XCTAssertEqual(JSON.escape("\n\t\r\u{08}\u{0C}"), "\"\\n\\t\\r\\b\\f\"")
        XCTAssertEqual(JSON.escape("\u{01}\u{1F}"), "\"\\u0001\\u001f\"")
        XCTAssertEqual(JSON.escape("em—dash·°"), "\"em—dash·°\"") // non-ASCII passes through
    }
}

final class RefsTests: XCTestCase {
    func testColConversions() {
        XCTAssertEqual(Refs.numToCol(1), "A")
        XCTAssertEqual(Refs.numToCol(26), "Z")
        XCTAssertEqual(Refs.numToCol(27), "AA")
        XCTAssertEqual(Refs.numToCol(16384), "XFD")
        XCTAssertEqual(Refs.colToNum("A"), 1)
        XCTAssertEqual(Refs.colToNum("XFD"), 16384)
        for n in [1, 5, 26, 27, 700, 16384] { XCTAssertEqual(Refs.colToNum(Substring(Refs.numToCol(n))), n) }
    }

    func testParseCell() {
        XCTAssertEqual(Refs.parseCell("B2")?.col, 2)
        XCTAssertEqual(Refs.parseCell("$B$2")?.row, 2)
        XCTAssertNil(Refs.parseCell("B"))
        XCTAssertNil(Refs.parseCell("2"))
        XCTAssertNil(Refs.parseCell("XFE1")) // col > max
        XCTAssertNil(Refs.parseCell("A1048577")) // row > max
    }

    func testParseTarget() {
        XCTAssertEqual(Refs.parseTarget("B2")?.kind, "cell")
        XCTAssertEqual(Refs.parseTarget("A1:C3")?.kind, "range")
        XCTAssertEqual(Refs.parseTarget("B:D")?.kind, "cols")
        XCTAssertEqual(Refs.parseTarget("2:9")?.kind, "rows")
        XCTAssertEqual(Refs.parseTarget("Sheet1!B2")?.sheet, "Sheet1")
        XCTAssertEqual(Refs.parseTarget("'Q3 Data'!B2")?.sheet, "Q3 Data")
        XCTAssertNil(Refs.parseTarget("nonsense"))
        XCTAssertNil(Refs.parseTarget("A1:B2:C3"))
        // reversed ranges normalise
        let r = Refs.parseTarget("C3:A1")
        XCTAssertEqual(r?.box?.c1, 1)
        XCTAssertEqual(r?.box?.r2, 3)
    }
}

final class ScalarTests: XCTestCase {
    func testKinds() {
        XCTAssertEqual(ScalarGrammar.parseScalar("").kind, "blank")
        XCTAssertEqual(ScalarGrammar.parseScalar("0.3").numberValue, 0.3)
        XCTAssertEqual(ScalarGrammar.parseScalar("1e3").numberValue, 1000)
        XCTAssertEqual(ScalarGrammar.parseScalar("TRUE").boolValue, true)
        XCTAssertEqual(ScalarGrammar.parseScalar("false").boolValue, false)
        XCTAssertEqual(ScalarGrammar.parseScalar("2026-07-04").kind, "date")
        XCTAssertEqual(ScalarGrammar.parseScalar("12:30").kind, "time")
        XCTAssertEqual(ScalarGrammar.parseScalar("2026-07-04T06:00").kind, "date")
        XCTAssertEqual(ScalarGrammar.parseScalar("#DIV/0!").stringValue, "#DIV/0!")
        XCTAssertEqual(ScalarGrammar.parseScalar("#n/a").stringValue, "#N/A") // upper-cased
        XCTAssertEqual(ScalarGrammar.parseScalar("Plain text").stringValue, "Plain text")
    }

    func testTextForms() {
        let forced = ScalarGrammar.parseScalar("'TRUE")
        XCTAssertEqual(forced.stringValue, "TRUE")
        XCTAssertTrue(forced.forced)
        let quoted = ScalarGrammar.parseScalar("\"a\"\"b\"")
        XCTAssertEqual(quoted.stringValue, "a\"b")
        XCTAssertTrue(quoted.quoted)
        let bad = ScalarGrammar.parseScalar("\"unterminated")
        XCTAssertEqual(bad.problem, "unterminated quoted text")
    }

    func testFormulaAndCached() {
        let f = ScalarGrammar.parseScalar("=SUM(B1:B3) :: 987.8")
        XCTAssertEqual(f.formula, "SUM(B1:B3)")
        XCTAssertEqual(f.cached?.numberValue, 987.8)
        let g = ScalarGrammar.parseScalar("=IF(B1>0,\"pos :: x\",\"neg\") :: \"pos\"")
        XCTAssertEqual(g.formula, "IF(B1>0,\"pos :: x\",\"neg\")") // :: inside quotes ignored
        XCTAssertEqual(g.cached?.stringValue, "pos")
        let cse = ScalarGrammar.parseScalar("{=TRANSPOSE(A1:B2)}")
        XCTAssertTrue(cse.cse)
        XCTAssertEqual(cse.formula, "TRANSPOSE(A1:B2)")
        let badCse = ScalarGrammar.parseScalar("{=broken")
        XCTAssertEqual(badCse.problem, "unterminated CSE array formula")
        let badCached = ScalarGrammar.parseScalar("=A1 :: =B1")
        XCTAssertEqual(badCached.cached?.kind, "invalid")
    }

    func testSplitCached() {
        XCTAssertEqual(ScalarGrammar.splitCached("a :: b :: c").head, "a :: b")
        XCTAssertNil(ScalarGrammar.splitCached("no separator").cached)
    }
}
