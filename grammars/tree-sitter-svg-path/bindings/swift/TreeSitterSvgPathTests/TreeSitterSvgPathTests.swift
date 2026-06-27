import XCTest
import SwiftTreeSitter
import TreeSitterSvgPath

final class TreeSitterSvgPathTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_svg_path())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading SVG Path Data grammar")
    }
}
