// swift-tools-version:5.9
import PackageDescription

// Root SPM manifest so the GridMD Swift implementation is consumable straight
// from the repository URL:
//
//   .package(url: "https://github.com/…/grid-md.git", branch: "main")
//   → .product(name: "GridMD", package: "grid-md")   // the library
//
// The library is pure Swift + Foundation (+ the system `Compression`
// framework for DEFLATE reads); no third-party dependencies, so a fresh clone
// builds with `swift build` alone.

let package = Package(
    name: "GridMD",
    platforms: [
        .macOS(.v13),
        .iOS(.v16),
    ],
    products: [
        .library(name: "GridMD", targets: ["GridMD"]),
        .executable(name: "gridmd", targets: ["GridMDCLI"]),
    ],
    targets: [
        .target(
            name: "GridMD",
            path: "swift/Sources/GridMD"
        ),
        .executableTarget(
            name: "GridMDCLI",
            dependencies: ["GridMD"],
            path: "swift/Sources/GridMDCLI"
        ),
        .testTarget(
            name: "GridMDTests",
            dependencies: ["GridMD"],
            path: "swift/Tests/GridMDTests"
        ),
    ]
)
