import type Parser from 'tree-sitter';

/**
 * The tree-sitter language object for the injected SVG transform-list grammar.
 *
 * @see {@linkcode https://tree-sitter.github.io/node-tree-sitter/interfaces/Parser.Language.html Parser.Language}
 *
 * @example
 * import Parser from "tree-sitter";
 * import SvgTransform from "tree-sitter-svg-transform";
 *
 * const parser = new Parser();
 * parser.setLanguage(SvgTransform);
 */
declare const binding: Parser.Language & {
	/** The syntax highlighting query for this grammar. */
	HIGHLIGHTS_QUERY?: string;

	/** The text objects query for this grammar. */
	TEXTOBJECTS_QUERY?: string;
};

export default binding;
