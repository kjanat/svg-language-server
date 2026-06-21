import XCTest
import SwiftTreeSitter
import TreeSitterSvg

final class TreeSitterSvgTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_svg())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading SVG grammar")
    }
}
