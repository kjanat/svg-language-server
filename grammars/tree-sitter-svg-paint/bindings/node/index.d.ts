type BaseNode = {
	type: string;
	named: boolean;
};

type ChildNode = {
	multiple: boolean;
	required: boolean;
	types: BaseNode[];
};

type NodeInfo =
	| (BaseNode & {
		subtypes: BaseNode[];
	})
	| (BaseNode & {
		fields?: { [name: string]: ChildNode };
		children?: ChildNode;
	});

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
declare const binding: {
	/**
	 * The inner language object.
	 * @private
	 */
	language: unknown;

	/**
	 * The content of the `node-types.json` file for this grammar.
	 *
	 * @see {@linkplain https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types Static Node Types}
	 */
	nodeTypeInfo: NodeInfo[];

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
