// swift-tools-version:5.3

import Foundation
import PackageDescription

var sources = ["src/parser.c"]
if FileManager.default.fileExists(atPath: "src/scanner.c") {
    sources.append("src/scanner.c")
}

let package = Package(
    name: "TreeSitterSvgPath",
    products: [
        .library(name: "TreeSitterSvgPath", targets: ["TreeSitterSvgPath"]),
    ],
    dependencies: [
        .package(name: "SwiftTreeSitter", url: "https://github.com/tree-sitter/swift-tree-sitter", from: "0.9.0"),
    ],
    targets: [
        .target(
            name: "TreeSitterSvgPath",
            dependencies: [],
            path: ".",
            sources: sources,
            resources: [
                .copy("queries")
            ],
            publicHeadersPath: "bindings/swift",
            cSettings: [.headerSearchPath("src")]
        ),
        .testTarget(
            name: "TreeSitterSvgPathTests",
            dependencies: [
                "SwiftTreeSitter",
                "TreeSitterSvgPath",
            ],
            path: "bindings/swift/TreeSitterSvgPathTests"
        )
    ],
    cLanguageStandard: .c11
)
