/**
 * @file SVG transform-list grammar for Tree-sitter (injected into transform attrs)
 * @author Kaj Kowalski <info@kajkowalski.nl>
 * @license MIT
 *
 * The transform-function sub-grammar evicted from tree-sitter-svg. The host
 * captures `transform` / `gradientTransform` / `patternTransform` values as one
 * opaque token; this grammar is injected over that range. Mirrors the path-data
 * eviction. Source: SVG 2 "transform property/attribute" + CSS Transforms.
 */
/// <reference types="tree-sitter-cli/dsl" />

const NUMBER_PATTERN = /[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?/;

export default grammar({
	name: 'svg_transform',

	extras: () => [],

	conflicts: $ => [[$.transform_list]],

	rules: {
		source_file: $ =>
			choice(
				seq(optional($.wsp), $.transform_list, optional($.wsp)),
				$.wsp,
			),

		transform_list: $ =>
			seq(
				$.transform_function,
				repeat(seq($.comma_wsp, $.transform_function)),
			),

		transform_function: $ =>
			choice(
				$.matrix_transform,
				$.translate_transform,
				$.scale_transform,
				$.rotate_transform,
				$.skew_x_transform,
				$.skew_y_transform,
			),

		matrix_transform: $ =>
			seq(
				'matrix',
				'(',
				optional($.wsp),
				$.number,
				$.comma_wsp,
				$.number,
				$.comma_wsp,
				$.number,
				$.comma_wsp,
				$.number,
				$.comma_wsp,
				$.number,
				$.comma_wsp,
				$.number,
				optional($.wsp),
				')',
			),

		translate_transform: $ =>
			seq(
				'translate',
				'(',
				optional($.wsp),
				$.number,
				optional(seq($.comma_wsp, $.number)),
				optional($.wsp),
				')',
			),

		scale_transform: $ =>
			seq(
				'scale',
				'(',
				optional($.wsp),
				$.number,
				optional(seq($.comma_wsp, $.number)),
				optional($.wsp),
				')',
			),

		rotate_transform: $ =>
			seq(
				'rotate',
				'(',
				optional($.wsp),
				$.number,
				optional(seq($.comma_wsp, $.number, $.comma_wsp, $.number)),
				optional($.wsp),
				')',
			),

		skew_x_transform: $ =>
			seq(
				'skewX',
				'(',
				optional($.wsp),
				$.number,
				optional($.wsp),
				')',
			),

		skew_y_transform: $ =>
			seq(
				'skewY',
				'(',
				optional($.wsp),
				$.number,
				optional($.wsp),
				')',
			),

		number: _ => token(NUMBER_PATTERN),

		comma_wsp: $ =>
			choice(
				seq(optional($.wsp), ',', optional($.wsp)),
				$.wsp,
			),

		wsp: _ => token(/[ \t\r\n]+/),
	},
});
