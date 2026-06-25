/**
 * @file SVG path-data grammar for Tree-sitter (injected into `d` attribute)
 * @author Kaj Kowalski <info@kajkowalski.nl>
 * @license MIT
 *
 * The rich path sub-grammar evicted from tree-sitter-svg. The host grammar
 * captures the `d` value as one opaque `path_data_payload` token; this grammar
 * is injected over that range. Tokenisation that is hostile to the host DFA
 * (single-char commands glued to numbers, `50z`, `h1`) lives here in isolation,
 * so the host can never mis-munge a command/number seam.
 *
 * Source: SVG 2 "The grammar for path data" (https://svgwg.org/svg2-draft/paths.html).
 */
/// <reference types="tree-sitter-cli/dsl" />

import { PATH_COMMAND_LETTERS } from '#constants';

// Derived from the catalog-mirrored letter set so the generic command token and
// the drift guard share a single source of truth (see constants.js / grammar.test.ts).
const PATH_COMMAND = new RegExp(`[${PATH_COMMAND_LETTERS.join('')}]`);
const NUMBER_PATTERN = /[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?/;

export default grammar({
	name: 'svg_path',

	externals: $ => [$._number_continuation],

	extras: () => [],

	rules: {
		// A `d` payload is either real path data (optionally wsp-padded) or
		// whitespace only (`d="   "` is spec-valid, SVG 2 §9.3.1).
		source_file: $ =>
			choice(
				seq(optional($.path_wsp), $.path_data, optional($.path_wsp)),
				$.path_wsp,
			),

		path_data: $ =>
			prec.right(seq(
				$.moveto_segment,
				repeat(choice($.path_segment, seq($.path_wsp, $.path_segment))),
				optional($.path_wsp),
			)),

		path_segment: $ =>
			choice(
				$.closepath_segment,
				$.moveto_segment,
				$.implicit_lineto_segment,
				$.lineto_segment,
				$.horizontal_lineto_segment,
				$.vertical_lineto_segment,
				$.curveto_segment,
				$.smooth_curveto_segment,
				$.quadratic_bezier_curveto_segment,
				$.smooth_quadratic_bezier_curveto_segment,
				$.elliptical_arc_segment,
			),

		moveto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.moveto_command, $.path_command)),
					optional($.path_wsp),
					$.path_coordinate_pair,
					repeat(seq(optional($.path_comma_wsp), $.path_coordinate_pair)),
				),
			),

		closepath_segment: $ => field('command', alias($.closepath_command, $.path_command)),

		implicit_lineto_segment: $ =>
			prec.left(
				seq(
					$.path_coordinate_pair,
					repeat(seq(optional($.path_comma_wsp), $.path_coordinate_pair)),
				),
			),

		lineto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.lineto_command, $.path_command)),
					optional($.path_wsp),
					$.path_coordinate_pair,
					repeat(seq(optional($.path_comma_wsp), $.path_coordinate_pair)),
				),
			),

		horizontal_lineto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.horizontal_lineto_command, $.path_command)),
					optional($.path_wsp),
					$.path_coordinate,
					repeat(seq($._number_continuation, $.path_coordinate)),
				),
			),

		vertical_lineto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.vertical_lineto_command, $.path_command)),
					optional($.path_wsp),
					$.path_coordinate,
					repeat(seq($._number_continuation, $.path_coordinate)),
				),
			),

		curveto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.curveto_command, $.path_command)),
					optional($.path_wsp),
					$.curveto_argument,
					repeat(seq($._number_continuation, $.curveto_argument)),
				),
			),

		smooth_curveto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.smooth_curveto_command, $.path_command)),
					optional($.path_wsp),
					$.smooth_curveto_argument,
					repeat(seq($._number_continuation, $.smooth_curveto_argument)),
				),
			),

		quadratic_bezier_curveto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.quadratic_bezier_curveto_command, $.path_command)),
					optional($.path_wsp),
					$.quadratic_bezier_curveto_argument,
					repeat(seq($._number_continuation, $.quadratic_bezier_curveto_argument)),
				),
			),

		smooth_quadratic_bezier_curveto_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.smooth_quadratic_bezier_curveto_command, $.path_command)),
					optional($.path_wsp),
					$.path_coordinate_pair,
					repeat(seq($._number_continuation, $.path_coordinate_pair)),
				),
			),

		elliptical_arc_segment: $ =>
			prec.left(
				seq(
					field('command', alias($.elliptical_arc_command, $.path_command)),
					optional($.path_wsp),
					$.elliptical_arc_argument,
					repeat(seq($._number_continuation, $.elliptical_arc_argument)),
				),
			),

		curveto_argument: $ =>
			seq(
				$.path_coordinate_pair,
				optional($.path_comma_wsp),
				$.path_coordinate_pair,
				optional($.path_comma_wsp),
				$.path_coordinate_pair,
			),

		smooth_curveto_argument: $ =>
			seq(
				$.path_coordinate_pair,
				optional($.path_comma_wsp),
				$.path_coordinate_pair,
			),

		quadratic_bezier_curveto_argument: $ =>
			seq(
				$.path_coordinate_pair,
				optional($.path_comma_wsp),
				$.path_coordinate_pair,
			),

		elliptical_arc_argument: $ =>
			seq(
				$.elliptical_arc_radii,
				optional($.path_comma_wsp),
				$.path_rotation,
				optional($.path_comma_wsp),
				$.path_arc_flag,
				optional($.path_comma_wsp),
				$.path_sweep_flag,
				optional($.path_comma_wsp),
				$.path_coordinate_pair,
			),

		elliptical_arc_radii: $ =>
			seq(
				$.path_coordinate,
				optional($.path_comma_wsp),
				$.path_coordinate,
			),

		path_coordinate_pair: $ =>
			seq(
				$.path_coordinate,
				optional($.path_comma_wsp),
				$.path_coordinate,
			),

		path_coordinate: $ => $.path_number,

		path_rotation: $ => $.path_number,

		path_arc_flag: _ => token(/[01]/),

		path_sweep_flag: _ => token(/[01]/),

		path_comma_wsp: $ =>
			choice(
				seq(optional($.path_wsp), $.path_comma, optional($.path_wsp)),
				$.path_wsp,
			),

		moveto_command: _ => token(/[Mm]/),
		closepath_command: _ => token(/[Zz]/),
		lineto_command: _ => token(/[Ll]/),
		horizontal_lineto_command: _ => token(/[Hh]/),
		vertical_lineto_command: _ => token(/[Vv]/),
		curveto_command: _ => token(/[Cc]/),
		smooth_curveto_command: _ => token(/[Ss]/),
		quadratic_bezier_curveto_command: _ => token(/[Qq]/),
		smooth_quadratic_bezier_curveto_command: _ => token(/[Tt]/),
		elliptical_arc_command: _ => token(/[Aa]/),

		path_command: _ => token(PATH_COMMAND),
		path_number: _ => token(NUMBER_PATTERN),
		path_comma: _ => ',',
		path_wsp: _ => token(prec(1, /[ \t\r\n]+/)),
	},
});
