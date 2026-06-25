/**
 * @file SVG paint/color grammar for Tree-sitter (injected into paint attrs)
 * @author Kaj Kowalski <info@kajkowalski.nl>
 * @license MIT
 *
 * The paint + color-value sub-grammar evicted from tree-sitter-svg. The host
 * captures `fill` / `stroke` / `stop-color` / `flood-color` / … values (the
 * ATTRIBUTE_BUCKETS.color bucket) as one opaque token; this grammar is injected
 * over that range. Same pattern as path data and transform lists.
 *
 * Sources: SVG 2 "Painting" + CSS Color 4/5. Token lists mirror the catalog
 * (crates/svg-data); keep in sync if the catalog's color-space sets change.
 */
/// <reference types="tree-sitter-cli/dsl" />

import { ANGLE_UNITS, COLOR_INTERPOLATION_SPACES, COLOR_SPACES, HUE_INTERPOLATION_METHODS } from '#constants';

const NUMBER_PATTERN = /[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?/;

export default grammar({
	name: 'svg_paint',

	extras: () => [],

	conflicts: $ => [[$.paint_server]],

	supertypes: $ => [$.color_value],

	// Single-use hidden color-function wrappers: inlining removes a node-type and
	// a few LR states with no visible-tree change (the multi-use `_color_*`
	// component rules stay shared — inlining them explodes the state count).
	inline: $ => [
		$._rgb_color,
		$._hsl_color,
		$._hwb_color,
		$._lab_color,
		$._oklab_color,
		$._lch_color,
		$._oklch_color,
		$._color_function,
		$._color_mix_function,
	],

	rules: {
		source_file: $ =>
			choice(
				seq(optional($.wsp), $.paint_value, optional($.wsp)),
				$.wsp,
			),

		paint_value: $ =>
			choice(
				'none',
				'currentColor',
				'context-fill',
				'context-stroke',
				'inherit',
				$.paint_server,
				$.color_value,
			),

		paint_server: $ =>
			seq(
				'url(',
				optional($.wsp),
				$.iri_reference,
				optional($.wsp),
				')',
				optional(seq($.wsp, choice($.color_value, 'none', 'currentColor'))),
			),

		color_value: $ => choice($.hex_color, $.functional_color, $.named_color),

		hex_color: _ => token(/#(?:[0-9A-Fa-f]{3}|[0-9A-Fa-f]{4}|[0-9A-Fa-f]{6}|[0-9A-Fa-f]{8})/),

		functional_color: $ =>
			choice(
				$._rgb_color,
				$._hsl_color,
				$._hwb_color,
				$._lab_color,
				$._lch_color,
				$._oklab_color,
				$._oklch_color,
				$._color_function,
				$._color_mix_function,
			),

		_rgb_color: $ =>
			seq(
				alias(token(choice('rgb', 'rgba')), $.color_function_name),
				'(',
				optional($.wsp),
				$.number_or_percentage,
				$.comma_wsp,
				$.number_or_percentage,
				$.comma_wsp,
				$.number_or_percentage,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_hsl_color: $ =>
			seq(
				alias(token(choice('hsl', 'hsla')), $.color_function_name),
				'(',
				optional($.wsp),
				$.hue_value,
				$.comma_wsp,
				$.number_or_percentage,
				$.comma_wsp,
				$.number_or_percentage,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_hwb_color: $ =>
			seq(
				alias(token('hwb'), $.color_function_name),
				'(',
				optional($.wsp),
				$._color_hue_component,
				$.comma_wsp,
				$._color_component,
				$.comma_wsp,
				$._color_component,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_lab_color: $ =>
			seq(
				alias(token('lab'), $.color_function_name),
				'(',
				optional($.wsp),
				$._color_component,
				$.comma_wsp,
				$._color_component,
				$.comma_wsp,
				$._color_component,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_oklab_color: $ =>
			seq(
				alias(token('oklab'), $.color_function_name),
				'(',
				optional($.wsp),
				$._color_component,
				$.comma_wsp,
				$._color_component,
				$.comma_wsp,
				$._color_component,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_lch_color: $ =>
			seq(
				alias(token('lch'), $.color_function_name),
				'(',
				optional($.wsp),
				$._color_component,
				$.comma_wsp,
				$._color_component,
				$.comma_wsp,
				$._color_hue_component,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_oklch_color: $ =>
			seq(
				alias(token('oklch'), $.color_function_name),
				'(',
				optional($.wsp),
				$._color_component,
				$.comma_wsp,
				$._color_component,
				$.comma_wsp,
				$._color_hue_component,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_color_function: $ =>
			seq(
				alias(token('color'), $.color_function_name),
				'(',
				optional($.wsp),
				$.color_colorspace,
				$.wsp,
				$._color_component,
				$.wsp,
				$._color_component,
				$.wsp,
				$._color_component,
				optional($._color_alpha),
				optional($.wsp),
				')',
			),

		_color_mix_function: $ =>
			seq(
				alias(token('color-mix'), $.color_function_name),
				'(',
				optional($.wsp),
				$.color_interpolation_method,
				$.comma_wsp,
				$.color_mix_component,
				$.comma_wsp,
				$.color_mix_component,
				optional($.wsp),
				')',
			),

		color_interpolation_method: $ =>
			prec.right(seq(
				alias(token('in'), $.color_mix_in_keyword),
				$.wsp,
				$.color_interpolation_space,
				optional(seq($.wsp, $.color_hue_interpolation)),
			)),

		color_colorspace: _ => token(choice(...COLOR_SPACES)),

		color_interpolation_space: _ => token(choice(...COLOR_INTERPOLATION_SPACES)),

		color_hue_interpolation: $ =>
			seq(
				alias(
					token(choice(...HUE_INTERPOLATION_METHODS)),
					$.color_hue_direction,
				),
				$.wsp,
				alias(token('hue'), $.color_hue_keyword),
			),

		color_mix_component: $ =>
			prec.right(seq(
				$.color_value,
				optional(seq($.wsp, $.percentage)),
			)),

		_color_component: $ =>
			choice(
				$.number_or_percentage,
				alias(token('none'), $.color_none),
			),

		_color_hue_component: $ =>
			choice(
				$.hue_value,
				alias(token('none'), $.color_none),
			),

		_color_alpha: $ =>
			choice(
				seq($.comma_wsp, $.number_or_percentage),
				seq(
					optional($.wsp),
					'/',
					optional($.wsp),
					choice(
						$.number_or_percentage,
						alias(token('none'), $.color_none),
					),
				),
			),

		hue_value: $ => seq($.number, optional($.angle_unit)),

		angle_unit: _ => token(choice(...ANGLE_UNITS)),

		named_color: _ => token(/[A-Za-z][A-Za-z-]*/),

		number_or_percentage: $ => choice($.number, $.percentage),

		percentage: $ => seq($.number, '%'),

		number: _ => token(NUMBER_PATTERN),

		iri_reference: _ => token(prec(-1, /(?:#[A-Za-z_:][A-Za-z0-9_.:-]*|[^)\s"']+)/)),

		comma_wsp: $ => choice(seq(optional($.wsp), ',', optional($.wsp)), $.wsp),

		wsp: _ => token(/[ \t\r\n]+/),
	},
});
