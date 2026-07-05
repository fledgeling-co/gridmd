import XCTest
@testable import GridMD

final class ModelDumpTests: XCTestCase {
    func dump(_ body: String) throws -> String { try GridMD.dump("---\ngridmd: \"1.0\"\ntitle: T\n---\n# S\n\(body)") }

    func testBodyContentForms() throws {
        let d = try dump("""
        @ A1
          value: 42
        @ A2
          value: hello
        @ A3
          value: 2026-07-04
        @ A4
          value: true
        @ B1
          formula: =SUM(C:C)
          value: 5
        @ B2
          entity: { type: stock, id: "XNAS:MSFT", text: "MSFT" }
          fields: { Price: 1 }
        @ B3
          rich:
            - { text: "Hello ", bold: true }
            - { text: "World" }
        """)
        XCTAssertTrue(d.contains("\"v\": 42"))
        XCTAssertTrue(d.contains("\"v\": \"hello\""))
        XCTAssertTrue(d.contains("\"v\": \"2026-07-04\""))
        XCTAssertTrue(d.contains("\"t\": \"d\""))
        XCTAssertTrue(d.contains("\"v\": true"))
        XCTAssertTrue(d.contains("\"f\": \"SUM(C:C)\""))
        XCTAssertTrue(d.contains("\"v\": 5"))
        XCTAssertTrue(d.contains("\"v\": \"MSFT\""))
        XCTAssertTrue(d.contains("\"t\": \"rich\""))
        XCTAssertTrue(d.contains("\"v\": \"Hello World\""))
    }

    func testValueTimeAndEntityIdFallback() throws {
        let d = try dump("""
        @ A1
          value: 12:30
        @ A2
          entity: { type: stock, id: "ONLY-ID" }
        """)
        XCTAssertTrue(d.contains("\"t\": \"d\"")) // time value
        XCTAssertTrue(d.contains("\"v\": \"ONLY-ID\"")) // entity falls back to id
    }

    func testFormulaBodyWithSpillAndArray() throws {
        let d = try dump("""
        @ A1
          formula: =SORT(B:B)
          spill: A1:A3
        @ C1
          formula: =X
          array: C1:D2
        """)
        XCTAssertTrue(d.contains("\"array\": \"A1:A3\""))
        XCTAssertTrue(d.contains("\"array\": \"C1:D2\""))
    }

    func testTranslateFormula() {
        XCTAssertEqual(translateFormula("A1*2", 1, 1), "B2*2")
        XCTAssertEqual(translateFormula("$A$1+B2", 1, 1), "$A$1+C3")
        XCTAssertEqual(translateFormula("SUM(A1)", 0, 1), "SUM(B1)")
        XCTAssertEqual(translateFormula("\"A1\"+A1", 0, 1), "\"A1\"+B1")
        XCTAssertEqual(translateFormula("A1B+A1", 0, 1), "A1B+B1") // A1B not a ref
        XCTAssertEqual(translateFormula("'sheet name'!A1", 0, 1), "'sheet name'!B1")
    }

    func testRelativeFillDump() throws {
        let d = try dump("@ B2:C3 =A1*2")
        XCTAssertTrue(d.contains("\"f\": \"A1*2\"")) // B2
        XCTAssertTrue(d.contains("\"f\": \"B1*2\"")) // C2
        XCTAssertTrue(d.contains("\"f\": \"A2*2\"")) // B3
        XCTAssertTrue(d.contains("\"f\": \"B2*2\"")) // C3
    }

    func testChartSheetHasNoCells() throws {
        let d = try GridMD.dump("""
        ---
        gridmd: "1.0"
        ---
        # Chart
        ```{sheet}
        kind: chart
        ```
        ```{chart} column at sheet
        series:
          - { val: A1 }
        ```
        """)
        XCTAssertTrue(d.contains("\"kind\": \"chart\""))
        XCTAssertTrue(d.contains("\"cells\": {}"))
        XCTAssertTrue(d.contains("\"charts\": 1"))
    }

    func testHiddenVeryAndProtected() throws {
        let d = try GridMD.dump("""
        ---
        gridmd: "1.0"
        ---
        # S
        ```{sheet}
        hidden: very
        protect: { enabled: true }
        ```
        @ A1 1
        """)
        XCTAssertTrue(d.contains("\"hidden\": \"very\""))
        XCTAssertTrue(d.contains("\"protected\": true"))
    }
}
