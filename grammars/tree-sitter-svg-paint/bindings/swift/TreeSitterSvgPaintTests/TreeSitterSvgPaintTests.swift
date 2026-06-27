import XCTest
import SwiftTreeSitter
import TreeSitterSvgPaint

final class TreeSitterSvgPaintTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_svg_paint())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading SVG Paint and Color grammar")
    }
}
