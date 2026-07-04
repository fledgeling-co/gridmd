import XCTest
@testable import GridMD

/// The three conformance laws (conformance/README.md) against the shared fixtures.
final class ConformanceTests: XCTestCase {
    let validFixtures = [
        "conformance/fixtures/01-cells.gmd": "conformance/expected/01-cells.json",
        "conformance/fixtures/02-structure.gmd": "conformance/expected/02-structure.json",
        "conformance/fixtures/03-features.gmd": "conformance/expected/03-features.json",
        "examples/quarterly-report.gmd": "conformance/expected/quarterly-report.json",
    ]

    // Law 1: parse + dump byte-identical to expected.
    func testLaw1DumpByteIdentical() throws {
        for (gmd, expected) in validFixtures {
            let source = try Repo.text(gmd)
            let dump = try GridMD.dump(source)
            let want = try Repo.text(expected)
            XCTAssertEqual(dump, want, "dump mismatch for \(gmd)")
        }
    }

    // Law 2: every invalid fixture fails strict lint.
    func testLaw2RejectInvalid() throws {
        for name in ["bad-table-headers", "duplicate-cell", "orphan-spill-cache"] {
            let source = try Repo.text("conformance/invalid/\(name).gmd")
            let result = GridMD.lint(source, strict: true)
            XCTAssertFalse(result.errors.isEmpty, "\(name) should be rejected")
            XCTAssertThrowsError(try GridMD.dump(source)) { error in
                guard case GridMD.Failure.invalid = error else { return XCTFail("wrong error") }
            }
        }
    }

    // Law 3: dump(import(export(doc))) == dump(doc).
    func testLaw3RoundTrip() throws {
        for gmd in validFixtures.keys {
            let source = try Repo.text(gmd)
            let exported = try GridMD.exportXLSX(source)
            XCTAssertFalse(exported.report.isEmpty)
            let imported = try GridMD.importXLSX(exported.data)
            let rtDump = try GridMD.dump(imported.gmd)
            let origDump = try GridMD.dump(source)
            XCTAssertEqual(rtDump, origDump, "round-trip dump mismatch for \(gmd)")
        }
    }

    // The importer's output must itself pass strict lint.
    func testImportSelfChecks() throws {
        for gmd in validFixtures.keys {
            let source = try Repo.text(gmd)
            let exported = try GridMD.exportXLSX(source)
            let imported = try GridMD.importXLSX(exported.data)
            XCTAssertTrue(GridMD.lint(imported.gmd).isValid)
        }
    }

    // Valid fixtures lint clean (zero errors).
    func testValidFixturesLintClean() throws {
        for gmd in validFixtures.keys {
            let result = GridMD.lint(try Repo.text(gmd))
            XCTAssertTrue(result.errors.isEmpty, "\(gmd) has errors: \(result.errors)")
            XCTAssertGreaterThan(result.sheets, 0)
        }
    }
}
