import XCTest
@testable import GridMD

final class YamlTests: XCTestCase {
    func testEmptyIsEmptyMap() {
        XCTAssertEqual(Yaml.parse("").value, .map([]))
        XCTAssertEqual(Yaml.parse("   \n # comment\n").value, .map([]))
    }

    func testBlockMapTypes() {
        let v = Yaml.parse("gridmd: \"0.1\"\ndate-system: 1904\nflag: true\nx: null\nf: 0.6\nname: Q3 Board Pack").value
        XCTAssertEqual(v["gridmd"], .string("0.1"))
        XCTAssertEqual(v["date-system"], .int(1904))
        XCTAssertEqual(v["flag"], .bool(true))
        XCTAssertEqual(v["x"], .null)
        XCTAssertEqual(v["f"], .double(0.6))
        XCTAssertEqual(v["name"], .string("Q3 Board Pack"))
    }

    func testNestedMapsAndSeqs() {
        let src = """
        theme:
          colors: { accent1: "#1F3FA6", accent2: "#63BE7B" }
          fonts: { major: Inter }
        names:
          - { name: TaxRate, ref: "Assumptions!$B$2" }
          - { name: Regions, value: '{"AU","NZ","UK"}' }
        """
        let v = Yaml.parse(src).value
        XCTAssertEqual(v["theme"]?["colors"]?["accent1"], .string("#1F3FA6"))
        XCTAssertEqual(v["theme"]?["fonts"]?["major"], .string("Inter"))
        let names = v["names"]!.listItems
        XCTAssertEqual(names.count, 2)
        XCTAssertEqual(names[0]["name"], .string("TaxRate"))
        XCTAssertEqual(names[0]["ref"], .string("Assumptions!$B$2"))
        XCTAssertEqual(names[1]["value"], .string("{\"AU\",\"NZ\",\"UK\"}"))
    }

    func testSequenceOfCompactMaps() {
        let src = """
        - when: "> 5"
          format: { fill: "#E7F6E7" }
        - bars: { color: "#638EC6" }
        - icons: 3-arrows
        """
        let v = Yaml.parse(src).value
        XCTAssertTrue(v.isList)
        XCTAssertEqual(v.count, 3)
        XCTAssertEqual(v.listItems[0]["when"], .string("> 5"))
        XCTAssertEqual(v.listItems[0]["format"]?["fill"], .string("#E7F6E7"))
        XCTAssertEqual(v.listItems[2]["icons"], .string("3-arrows"))
    }

    func testFlowScalarWithColon() {
        // `B9:B11` must be one string, not a nested map.
        let v = Yaml.parse("spill: B9:B11").value
        XCTAssertEqual(v["spill"], .string("B9:B11"))
        let f = FlowParser.parseFlowMap("{ spill: B9:B11 }")
        XCTAssertEqual(f?["spill"], .string("B9:B11"))
    }

    func testFlowSeq() {
        let v = Yaml.parse("values: [open, done]").value
        XCTAssertEqual(v["values"], .list([.string("open"), .string("done")]))
        XCTAssertEqual(FlowParser.parseFlowSeq("[]"), .list([]))
        XCTAssertEqual(FlowParser.parseFlowSeq("[1, 2, 3]"), .list([.int(1), .int(2), .int(3)]))
    }

    func testBlockScalarLiteralClip() {
        let src = "note: |\n  A cell note.\n"
        XCTAssertEqual(Yaml.parse(src).value["note"], .string("A cell note.\n"))
    }

    func testBlockScalarMultiline() {
        let src = "text: |\n  line one\n  line two\nfont: { size: 11 }"
        let v = Yaml.parse(src).value
        XCTAssertEqual(v["text"], .string("line one\nline two\n"))
        XCTAssertEqual(v["font"]?["size"], .int(11))
    }

    func testBlockScalarChomping() {
        XCTAssertEqual(Yaml.parse("t: |-\n  x").value["t"], .string("x"))
        XCTAssertEqual(Yaml.parse("t: |+\n  x\n\n").value["t"], .string("x\n\n\n"))
        XCTAssertEqual(Yaml.parse("t: |2\n   x").value["t"], .string(" x\n"))
        XCTAssertEqual(Yaml.parse("t: |\n").value["t"], .string(""))
    }

    func testFoldedScalar() {
        let src = "t: >\n  a\n  b\n\n  c\n"
        XCTAssertEqual(Yaml.parse(src).value["t"], .string("a b\nc\n"))
    }

    func testComments() {
        let v = Yaml.parse("a: 1 # trailing\nb: hi # c").value
        XCTAssertEqual(v["a"], .int(1))
        XCTAssertEqual(v["b"], .string("hi"))
    }

    func testQuotedEscapes() {
        XCTAssertEqual(scalarLeaf("\"a\\nb\\t\\\"c\\\\\""), .string("a\nb\t\"c\\"))
        XCTAssertEqual(scalarLeaf("\"\\u0041\""), .string("A"))
        XCTAssertEqual(scalarLeaf("'O''Brien'"), .string("O'Brien"))
        XCTAssertEqual(scalarLeaf(""), .null)
    }

    func testDashAloneSequence() {
        let src = "-\n  a: 1\n- b: 2"
        let v = Yaml.parse(src).value
        XCTAssertEqual(v.count, 2)
        XCTAssertEqual(v.listItems[0]["a"], .int(1))
        XCTAssertEqual(v.listItems[1]["b"], .int(2))
    }

    func testTryProps() {
        XCTAssertNotNil(Yaml.tryProps("{ merge: true, style: hdr }"))
        XCTAssertNil(Yaml.tryProps("{ Bad: true }")) // non-ident key
        XCTAssertNil(Yaml.tryProps("{ a: null }")) // null value
        XCTAssertNil(Yaml.tryProps("{1,2,3}")) // array-constant, not props
        XCTAssertNil(Yaml.tryProps("not a map"))
        XCTAssertTrue(Yaml.isIdentKey("x-custom"))
        XCTAssertFalse(Yaml.isIdentKey("Nope"))
    }

    func testTruthyAndJsString() {
        XCTAssertTrue(YamlValue.list([]).isTruthy)
        XCTAssertTrue(YamlValue.map([]).isTruthy)
        XCTAssertFalse(YamlValue.string("").isTruthy)
        XCTAssertFalse(YamlValue.int(0).isTruthy)
        XCTAssertFalse(YamlValue.double(0).isTruthy)
        XCTAssertFalse(YamlValue.bool(false).isTruthy)
        XCTAssertFalse(YamlValue.null.isTruthy)
        XCTAssertEqual(YamlValue.int(5).jsString, "5")
        XCTAssertEqual(YamlValue.double(0.3).jsString, "0.3")
        XCTAssertEqual(YamlValue.bool(true).jsString, "true")
        XCTAssertEqual(YamlValue.null.jsString, "null")
        XCTAssertEqual(YamlValue.list([]).jsString, "")
    }
}
