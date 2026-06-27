import type Parser from 'tree-sitter';

/**
 * The tree-sitter language object for this grammar.
 *
 * @see {@linkcode https://tree-sitter.github.io/node-tree-sitter/interfaces/Parser.Language.html Parser.Language}
 *
 * @example
 * import Parser from "tree-sitter";
 * import Svg from "tree-sitter-svg";
 *
 * const parser = new Parser();
 * parser.setLanguage(Svg);
 */
declare const binding: Parser.Language & {
	/** The syntax highlighting query for this grammar. */
	HIGHLIGHTS_QUERY?: string;

	/** The language injection query for this grammar. */
	INJECTIONS_QUERY?: string;

	/** The local variable query for this grammar. */
	LOCALS_QUERY?: string;

	/** The symbol tagging query for this grammar. */
	TAGS_QUERY?: string;
};

export default binding;
