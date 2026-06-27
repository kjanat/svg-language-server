import type Parser from 'tree-sitter';

/**
 * The tree-sitter language object for the injected SVG paint/color grammar.
 *
 * @see {@linkcode https://tree-sitter.github.io/node-tree-sitter/interfaces/Parser.Language.html Parser.Language}
 *
 * @example
 * import Parser from "tree-sitter";
 * import SvgPaint from "tree-sitter-svg-paint";
 *
 * const parser = new Parser();
 * parser.setLanguage(SvgPaint);
 */
declare const binding: Parser.Language & {
	/** The syntax highlighting query for this grammar. */
	HIGHLIGHTS_QUERY?: string;

	/** The local variable query for this grammar. */
	LOCALS_QUERY?: string;

	/** The symbol tagging query for this grammar. */
	TAGS_QUERY?: string;

	/** The text objects query for this grammar. */
	TEXTOBJECTS_QUERY?: string;
};

export default binding;
