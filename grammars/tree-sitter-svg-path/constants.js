/**
 * @file Catalog-derived path-command letters for the svg_path grammar.
 * @license MIT
 *
 * Hardcoded on purpose so the grammar stays standalone (no catalog JSON pulled into grammar.js).
 * `grammar.test.ts` asserts this stays equal to the svg-data catalog
 * (crates/svg-data/data/catalog.tree-sitter.json tokens.path_command_letters)
 * — that test is the drift guard. Imported by both grammar.js and the test.
 *
 * Both-case letters for the 10 SVG path commands: M Z L H V C S Q T A.
 */
// @ts-check

export const PATH_COMMAND_LETTERS = /* dprint-ignore */ [
	'A', 'C', 'H', 'L', 'M', 'Q', 'S', 'T', 'V', 'Z',
	'a', 'c', 'h', 'l', 'm', 'q', 's', 't', 'v', 'z',
];
