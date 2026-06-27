import XCTest
import SwiftTreeSitter
import TreeSitterSvgTransform

final class TreeSitterSvgTransformTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_svg_transform())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading SVG Transform List grammar")
    }
}
