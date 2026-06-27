export const D_ATTRIBUTE_NAMES: readonly string[] = ['d', 'path'];

/**
 * Attribute names handled by dedicated grammar rules instead of generated
 * `ATTRIBUTE_BUCKETS` lists. Keep aligned with `svg-data-regen`.
 */
export const GRAMMAR_DEDICATED_ATTRIBUTE_NAMES: readonly string[] = [
	'class',
	'clip',
	'd',
	'dur',
	'enable-background',
	'gradientTransform',
	'href',
	'id',
	'keySplines',
	'keyTimes',
	'offset',
	'path',
	'patternTransform',
	'preserveAspectRatio',
	'repeatCount',
	'repeatDur',
	'rotate',
	'style',
	'transform',
	'xlink:href',
];

/** Attribute buckets consumed via `choice(...ATTRIBUTE_BUCKETS.*)` in grammar.js. */
export const GENERATED_ATTRIBUTE_BUCKET_KEYS: readonly string[] = [
	'keyword',
	'color',
	'length',
	'length_list',
	'length_list_or_none',
	'number',
	'number_list',
	'number_optional_number',
	'number_or_percentage',
	'coordinate_pair_list',
	'view_box',
	'functional_iri',
	'css_text',
];

/**
 * Token sets consumed via `TOKENS.*` in the host grammar.js.
 *
 * Path-command, color-space, hue and angle token sets were evicted to the
 * injected sibling grammars (tree-sitter-svg-path / tree-sitter-svg-paint),
 * which own their own catalog drift guards; the host no longer projects them.
 */
export const TOKEN_KEYS: readonly string[] = [
	'length_units',
	'time_units',
];
