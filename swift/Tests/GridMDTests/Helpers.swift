import Foundation
import XCTest
@testable import GridMD

/// Locates the repository root from this test file's path so the conformance
/// tests can read the shared fixtures/expected/examples.
enum Repo {
    static let root: URL = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // GridMDTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // swift
        .deletingLastPathComponent() // repo root

    static func text(_ relative: String) throws -> String {
        let url = root.appendingPathComponent(relative)
        return try String(contentsOf: url, encoding: .utf8)
    }
}
