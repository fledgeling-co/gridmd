import XCTest
import Compression
@testable import GridMD

/// Targeted tests closing coverage gaps in edge/error branches.
final class CoverageTests: XCTestCase {
    // Two tables + two names in one sheet exercise the dump's sort comparators.
    func testMultiTableAndNameSort() throws {
        let d = try GridMD.dump("""
        ---
        gridmd: "0.1"
        names:
          - { name: Zeta, ref: A1 }
          - { name: Alpha, ref: B1 }
        ---
        # S
        ```{table} Zebra at A1
        ---
        | a |
        | 1 |
        ```
        ```{table} Apple at C1
        ---
        | b |
        | 2 |
        ```
        """)
        // names sorted: Alpha before Zeta
        XCTAssertLessThan(d.range(of: "\"name\": \"Alpha\"")!.lowerBound, d.range(of: "\"name\": \"Zeta\"")!.lowerBound)
        // tables sorted: Apple before Zebra
        XCTAssertLessThan(d.range(of: "\"name\": \"Apple\"")!.lowerBound, d.range(of: "\"name\": \"Zebra\"")!.lowerBound)
    }

    // Whole-column @ target carries no content (Model early return).
    func testWholeColumnAtTarget() throws {
        let d = try GridMD.dump("---\ngridmd: \"0.1\"\n---\n# S\n@ B:B { fill: \"#FFFFFF\" }\n@ A1 1")
        XCTAssertTrue(d.contains("\"A1\""))
        XCTAssertFalse(d.contains("\"B1\""))
    }

    // Fence anchor qualified by a different sheet name → error.
    func testCrossSheetAnchorQualifier() {
        let errs = GridMD.lint("---\ngridmd: \"0.1\"\n---\n# S\n```{cf} Other!A1:B2\n- when: x\n```").errors
        XCTAssertTrue(errs.contains { $0.message.contains("must name the containing sheet") })
    }

    // Relative fill over the enumeration cap emits a warning instead of enumerating.
    func testRelativeFillCapWarning() {
        let result = GridMD.lint("---\ngridmd: \"0.1\"\n---\n# S\n@ A1:A20000 =B1")
        XCTAssertTrue(result.errors.isEmpty)
        XCTAssertTrue(result.warnings.contains { $0.message.contains("overlap checking skipped") })
    }

    func testFlowMapQuotedKeys() {
        let v = FlowParser.parseFlowMap("{ \"a:b\": 1, 'c': 2 }")
        XCTAssertEqual(v?["a:b"], .int(1))
        XCTAssertEqual(v?["c"], .int(2))
    }

    func testDoubleQuoteEscapeFallbacks() {
        XCTAssertEqual(scalarLeaf("\"\\q\""), .string("q")) // unknown escape → literal
        XCTAssertEqual(scalarLeaf("\"\\uZZZZ\""), .string("uZZZZ")) // invalid \u → fallback
    }

    func testYamlSubscriptOnNonMap() {
        XCTAssertNil(YamlValue.string("x")["key"])
        XCTAssertNil(YamlValue.list([])["key"])
    }

    func testFindPropsSplitEdgeReturns() {
        XCTAssertNil(Parser.findPropsSplit("{x: 1}").propsText) // group at start (s == 0)
        XCTAssertNil(Parser.findPropsSplit("a{x: 1}").propsText) // not space-preceded
    }

    func testMalformedPipeRowInGrid() {
        let doc = Parser.parseDocument("---\ngridmd: \"0.1\"\n---\n# S\n```{grid} A1\n| 1 |\nnot a row\n```")
        XCTAssertTrue(doc.errors.contains { $0.msg.contains("expected a pipe row") })
    }

    func testAtBodyNotAMapping() {
        let doc = Parser.parseDocument("---\ngridmd: \"0.1\"\n---\n# S\n@ A1\n  - item")
        XCTAssertTrue(doc.errors.contains { $0.msg.contains("must be a YAML mapping") })
    }

    func testRowRangeOutOfBounds() {
        XCTAssertNil(Refs.parseTarget("1:1048577")) // r2 > maxRow
        XCTAssertNil(Refs.parseTarget("XFE:XFF")) // cols > maxCol
    }

    func testZipEocdScanWithTrailingBytes() throws {
        var zip = Zip.write([ZipEntry(name: "a", data: Array("hi".utf8))])
        zip.append(0) // trailing byte after EOCD forces the back-scan to decrement
        let read = try Zip.read(zip)
        XCTAssertEqual(String(decoding: read[0].data, as: UTF8.self), "hi")
    }

    func testInflateGarbageThrows() {
        XCTAssertThrowsError(try Zip.inflateRaw([0x00, 0x11, 0x22], expected: 100, name: "x"))
    }
}
