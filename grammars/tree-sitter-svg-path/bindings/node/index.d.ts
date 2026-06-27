import type Parser from 'tree-sitter';

/**
 * The tree-sitter language object for the injected SVG path-data grammar.
 *
 * @see {@linkcode https://tree-sitter.github.io/node-tree-sitter/interfaces/Parser.Language.html Parser.Language}
 *
 * @example
 * import Parser from "tree-sitter";
 * import SvgPath from "tree-sitter-svg-path";
 *
 * const parser = new Parser();
 * parser.setLanguage(SvgPath);
 */
declare const binding: Parser.Language & {
	/** The syntax highlighting query for this grammar. */
	HIGHLIGHTS_QUERY?: string;

	/** The text objects query for this grammar. */
	TEXTOBJECTS_QUERY?: string;
};

export default binding;
