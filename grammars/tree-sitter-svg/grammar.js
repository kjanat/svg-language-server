/**
 * @file SVG grammar for Tree-sitter
 * @author Kaj Kowalski <info@kajkowalski.nl>
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PATH_COMMAND = /[MmZzLlHhVvCcSsQqTtAa]/;
const NUMBER_PATTERN = /[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?/;

/** @param {RuleOrLiteral} value */
function quoted(value) {
	return choice(
		seq('"', optional(value), '"'),
		seq("'", optional(value), "'"),
	);
}

export default grammar({
	name: 'svg',

	externals: $ => [
		$._start_tag_name,
		$._path_start_tag_name,
		$._script_start_tag_name,
		$._style_start_tag_name,
		$._end_tag_name,
		$._path_end_tag_name,
		$._script_end_tag_name,
		$._style_end_tag_name,
		$._erroneous_end_tag_name,
		$._raw_text,
		'/>',
		$._cdata_text,
		$._number_continuation,
	],

	extras: () => [],

	rules: {
		source_file: $ =>
			seq(
				// XML 1.0 §2.8 requires xml_declaration to be absolute first,
				// but real-world SVGs (incl. W3C reference samples) often
				// have a leading newline/BOM/whitespace; be lenient here.
				// Leading misc only permitted when an xml_declaration follows,
				// to keep source_file_repeat1 unambiguous.
				optional(seq(repeat($._misc), $.xml_declaration)),
				repeat($._misc),
				optional(seq($.doctype, repeat($._misc))),
				field('root', $.svg_root_element),
				repeat($._misc),
			),

		_misc: $ =>
			choice(
				$.processing_instruction,
				$.comment,
				alias($.misc_text, $.text),
			),

		misc_text: _ => token(prec(-1, /[ \t\r\n]+/)),

		// ─── Document Nodes ─────────────────────────────────────────

		xml_declaration: $ =>
			seq(
				// Start token consumes `<?xml` + mandatory whitespace. The
				// trailing ws disambiguates from PI targets that begin with
				// `xml` (e.g. `<?xml-stylesheet`), which XML 1.0 §2.8 and the
				// pi_target_name regex reserve as valid PIs.
				$._xml_declaration_start,
				$.xml_version_attribute,
				optional(seq($._s, $.xml_encoding_attribute)),
				optional(seq($._s, $.xml_standalone_attribute)),
				optional($._s),
				'?>',
			),

		_xml_declaration_start: _ => token(prec(2, /<\?xml[ \t\r\n]+/)),

		xml_version_attribute: $ =>
			seq(
				field('name', $.xml_version_attribute_name),
				$._eq,
				field('value', $.quoted_attribute_value),
			),

		xml_version_attribute_name: _ => 'version',

		xml_encoding_attribute: $ =>
			seq(
				field('name', $.xml_encoding_attribute_name),
				$._eq,
				field('value', $.quoted_attribute_value),
			),

		xml_encoding_attribute_name: _ => 'encoding',

		xml_standalone_attribute: $ =>
			seq(
				field('name', $.xml_standalone_attribute_name),
				$._eq,
				field('value', $.xml_standalone_attribute_value),
			),

		xml_standalone_attribute_name: _ => 'standalone',
		xml_standalone_attribute_value: _ => choice('"yes"', '"no"', "'yes'", "'no'"),

		doctype: $ =>
			seq(
				'<!DOCTYPE',
				$._s,
				field('name', $.name),
				optional(seq($._s, field('external_id', $.doctype_external_id))),
				optional(seq($._s, field('internal_subset', $.doctype_internal_subset))),
				optional($._s),
				'>',
			),

		doctype_external_id: _ => token(/[^\x5B\x5D>]+/),

		doctype_internal_subset: _ => seq('[', token(/[^\]]*/), ']'),

		processing_instruction: $ =>
			seq(
				'<?',
				field('target', alias($.pi_target_name, $.name)),
				optional(seq($._s, field('content', $.pi_content))),
				'?>',
			),

		pi_target_name: _ =>
			token(choice(
				/[A-WYZa-wyz_:][A-Za-z0-9_.:-]*/,
				/[xX][A-LN-Za-ln-z0-9_.:-][A-Za-z0-9_.:-]*/,
				/[xX][mM][A-KM-Za-km-z0-9_.:-][A-Za-z0-9_.:-]*/,
				/[xX][mM][lL][A-Za-z0-9_.:-]+/,
			)),

		pi_content: _ => token(/([^?]|\?[^>])+/),

		comment: $ =>
			seq(
				'<!--',
				optional(field('text', $.comment_text)),
				'-->',
			),

		comment_text: _ =>
			repeat1(choice(
				token.immediate(/[^-]+/),
				token.immediate(/-[^-]/),
			)),

		cdata_section: $ =>
			seq(
				'<![CDATA[',
				optional($.cdata_text),
				']]>',
			),

		cdata_text: $ => repeat1($._cdata_text),

		// ─── SVG Root Element ───────────────────────────────────────

		svg_root_element: $ =>
			choice(
				$.self_closing_tag,
				seq(
					$.start_tag,
					repeat($._content),
					choice($.end_tag, $.erroneous_end_tag),
				),
			),

		// ─── Generic Element ────────────────────────────────────────

		element: $ =>
			choice(
				$._script_element,
				$._style_element,
				$._path_element,
				$.self_closing_tag,
				seq($.start_tag, repeat($._content), choice($.end_tag, $.erroneous_end_tag)),
			),

		_content: $ =>
			choice(
				$.element,
				$._text_like_content,
			),

		_text_like_content: $ =>
			choice(
				$.comment,
				$.processing_instruction,
				$.cdata_section,
				$.entity_reference,
				$.text,
			),

		// ─── Script Element (raw_text for JS injection) ─────────────

		_script_element: $ =>
			choice(
				alias($.script_self_closing_tag, $.self_closing_tag),
				seq(
					alias($.script_start_tag, $.start_tag),
					optional(alias($._raw_text, $.raw_text)),
					choice(alias($.script_end_tag, $.end_tag), $.erroneous_end_tag),
				),
			),

		script_start_tag: $ =>
			seq(
				'<',
				field('name', alias($._script_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'>',
			),

		script_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._script_end_tag_name, $.name)),
				optional($._s),
				'>',
			),

		script_self_closing_tag: $ =>
			seq(
				'<',
				field('name', alias($._script_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'/>',
			),

		// ─── Style Element (raw_text for CSS injection) ─────────────

		_style_element: $ =>
			choice(
				alias($.style_self_closing_tag, $.self_closing_tag),
				seq(
					alias($.style_element_start_tag, $.start_tag),
					optional(alias($._raw_text, $.raw_text)),
					choice(alias($.style_end_tag, $.end_tag), $.erroneous_end_tag),
				),
			),

		style_element_start_tag: $ =>
			seq(
				'<',
				field('name', alias($._style_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'>',
			),

		style_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._style_end_tag_name, $.name)),
				optional($._s),
				'>',
			),

		style_self_closing_tag: $ =>
			seq(
				'<',
				field('name', alias($._style_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'/>',
			),

		// ─── Path Element (d attribute sub-grammar) ─────────────────

		_path_element: $ =>
			choice(
				alias($.path_self_closing_tag, $.self_closing_tag),
				seq(
					alias($.path_start_tag, $.start_tag),
					repeat($._content),
					choice(alias($.path_end_tag, $.end_tag), $.erroneous_end_tag),
				),
			),

		path_start_tag: $ =>
			seq(
				'<',
				field('name', alias($._path_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'>',
			),

		path_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._path_end_tag_name, $.name)),
				optional($._s),
				'>',
			),

		path_self_closing_tag: $ =>
			seq(
				'<',
				field('name', alias($._path_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'/>',
			),

		// ─── Generic Tags ───────────────────────────────────────────

		start_tag: $ =>
			seq(
				'<',
				field('name', alias($._start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'>',
			),

		end_tag: $ =>
			seq(
				'</',
				field('name', alias($._end_tag_name, $.name)),
				optional($._s),
				'>',
			),

		self_closing_tag: $ =>
			seq(
				'<',
				field('name', alias($._start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'/>',
			),

		erroneous_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._erroneous_end_tag_name, $.name)),
				optional($._s),
				'>',
			),

		// ─── Attributes ─────────────────────────────────────────────

		attribute: $ =>
			choice(
				$._typed_attribute,
				$.generic_attribute,
			),

		_typed_attribute: $ =>
			choice(
				$.d_attribute,
				$.viewbox_attribute,
				$.preserve_aspect_ratio_attribute,
				$.transform_attribute,
				$.points_attribute,
				$.style_attribute,
				$.paint_attribute,
				$.functional_iri_attribute,
				$.clip_attribute,
				$.opacity_attribute,
				$.length_attribute,
				$.offset_attribute,
				$.number_attribute,
				$.length_list_attribute,
				$.stroke_dasharray_attribute,
				$.number_list_attribute,
				$.duration_attribute,
				$.repeat_count_attribute,
				$.key_times_attribute,
				$.key_splines_attribute,
				$.enable_background_attribute,
				$.href_attribute,
				$.id_attribute,
				$.class_attribute,
				$.event_attribute,
			),

		// ─── d attribute (path data sub-grammar) ────────────────────

		d_attribute: $ =>
			seq(
				field('name', $.d_attribute_name),
				$._eq,
				field('value', $.d_attribute_value),
			),

		d_attribute_name: _ => choice('d', 'path'),

		d_attribute_value: $ =>
			choice(
				$.double_quoted_path_data,
				$.single_quoted_path_data,
			),

		double_quoted_path_data: $ =>
			seq(
				'"',
				optional($.path_data_content),
				'"',
			),

		single_quoted_path_data: $ =>
			seq(
				"'",
				optional($.path_data_content),
				"'",
			),

		path_data_content: $ =>
			choice(
				seq(optional($.path_wsp), $.path_data),
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

		// ─── style attribute (CSS injection) ────────────────────────

		style_attribute: $ =>
			seq(
				field('name', $.style_attribute_name),
				$._eq,
				field('value', $.style_attribute_value),
			),

		style_attribute_name: _ => 'style',

		style_attribute_value: $ =>
			choice(
				$.double_quoted_style_value,
				$.single_quoted_style_value,
			),

		double_quoted_style_value: $ =>
			seq(
				'"',
				optional(field('content', $.style_text_double)),
				'"',
			),

		single_quoted_style_value: $ =>
			seq(
				"'",
				optional(field('content', $.style_text_single)),
				"'",
			),

		style_text_double: _ => token(/[^"]+/),
		style_text_single: _ => token(/[^']+/),

		// ─── viewBox attribute ──────────────────────────────────────

		viewbox_attribute: $ =>
			seq(
				field('name', $.viewbox_attribute_name),
				$._eq,
				field('value', $.viewbox_attribute_value),
			),

		viewbox_attribute_name: _ => 'viewBox',

		viewbox_attribute_value: $ => quoted($.viewbox_value),

		viewbox_value: $ =>
			seq(
				$.number,
				$.comma_wsp,
				$.number,
				$.comma_wsp,
				$.number,
				$.comma_wsp,
				$.number,
			),

		// ─── preserveAspectRatio attribute ──────────────────────────

		preserve_aspect_ratio_attribute: $ =>
			seq(
				field('name', $.preserve_aspect_ratio_attribute_name),
				$._eq,
				field('value', $.preserve_aspect_ratio_attribute_value),
			),

		preserve_aspect_ratio_attribute_name: _ => 'preserveAspectRatio',

		preserve_aspect_ratio_attribute_value: $ => quoted($.preserve_aspect_ratio_value),

		preserve_aspect_ratio_value: $ =>
			seq(
				optional(seq('defer', $.wsp)),
				$.align_keyword,
				optional(seq($.wsp, $.meet_or_slice_keyword)),
			),

		align_keyword: _ =>
			choice(
				'none',
				'xMinYMin',
				'xMidYMin',
				'xMaxYMin',
				'xMinYMid',
				'xMidYMid',
				'xMaxYMid',
				'xMinYMax',
				'xMidYMax',
				'xMaxYMax',
			),

		meet_or_slice_keyword: _ => choice('meet', 'slice'),

		// ─── transform attribute ────────────────────────────────────

		transform_attribute: $ =>
			seq(
				field('name', $.transform_attribute_name),
				$._eq,
				field('value', $.transform_attribute_value),
			),

		transform_attribute_name: _ =>
			choice(
				'transform',
				'gradientTransform',
				'patternTransform',
			),

		transform_attribute_value: $ => quoted($.transform_list),

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

		// ─── points attribute ───────────────────────────────────────

		points_attribute: $ =>
			seq(
				field('name', $.points_attribute_name),
				$._eq,
				field('value', $.points_attribute_value),
			),

		points_attribute_name: _ => 'points',

		points_attribute_value: $ => quoted($.coordinate_pair_list),

		coordinate_pair_list: $ =>
			seq(
				$.coordinate_pair,
				repeat(seq($.comma_wsp, $.coordinate_pair)),
			),

		coordinate_pair: $ => seq($.number, optional($.comma_wsp), $.number),

		// ─── paint attribute (fill, stroke, color, etc.) ────────────

		paint_attribute: $ =>
			seq(
				field('name', $.paint_attribute_name),
				$._eq,
				field('value', $.paint_attribute_value),
			),

		paint_attribute_name: _ =>
			choice(
				'fill',
				'stroke',
				'color',
				'stop-color',
				'flood-color',
				'lighting-color',
			),

		paint_attribute_value: $ => quoted($.paint_value),

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

		// rgb() / rgba() — 3 components + optional /alpha
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

		// hsl() / hsla() — hue + 2 percentages + optional /alpha
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

		// hwb() — hue + 2 components + optional /alpha (CSS Color 4)
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

		// lab() — L a b + optional /alpha (CSS Color 4)
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

		// oklab() — L a b + optional /alpha (CSS Color 4)
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

		// lch() — L C H + optional /alpha (CSS Color 4)
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

		// oklch() — L C H + optional /alpha (CSS Color 4)
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

		// color(<colorspace> c1 c2 c3 [/alpha]?) — CSS Color 4
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

		// color-mix(in <cs> [hue-method]?, <color> [pct]?, <color> [pct]?) — CSS Color 5
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

		// Predefined color() colorspaces (rectangular, XYZ).
		color_colorspace: _ =>
			token(choice(
				'srgb-linear',
				'srgb',
				'display-p3',
				'a98-rgb',
				'prophoto-rgb',
				'rec2020',
				'xyz-d50',
				'xyz-d65',
				'xyz',
			)),

		// color-mix interpolation spaces (rectangular + polar + predefined).
		color_interpolation_space: _ =>
			token(choice(
				'srgb-linear',
				'srgb',
				'display-p3',
				'a98-rgb',
				'prophoto-rgb',
				'rec2020',
				'xyz-d50',
				'xyz-d65',
				'xyz',
				'hsl',
				'hwb',
				'lab',
				'lch',
				'oklab',
				'oklch',
			)),

		color_hue_interpolation: $ =>
			seq(
				alias(
					token(choice('shorter', 'longer', 'increasing', 'decreasing')),
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

		angle_unit: _ => choice('deg', 'rad', 'grad', 'turn'),

		named_color: _ => token(/[A-Za-z][A-Za-z-]*/),

		// ─── functional IRI attribute (url(#ref)) ───────────────────

		functional_iri_attribute: $ =>
			seq(
				field('name', $.functional_iri_attribute_name),
				$._eq,
				field('value', $.functional_iri_attribute_value),
			),

		functional_iri_attribute_name: _ =>
			choice(
				'clip-path',
				'mask',
				'filter',
				'marker-start',
				'marker-mid',
				'marker-end',
				'cursor',
			),

		functional_iri_attribute_value: $ => quoted(choice('none', $.functional_iri, $.iri_reference)),

		functional_iri: $ => seq('url(', optional($.wsp), $.iri_reference, optional($.wsp), ')'),

		// ─── clip attribute (deprecated, rect() function) ───────────

		clip_attribute: $ =>
			seq(
				field('name', $.clip_attribute_name),
				$._eq,
				field('value', $.clip_attribute_value),
			),

		clip_attribute_name: _ => 'clip',

		clip_attribute_value: $ => quoted(choice('auto', 'inherit', $.clip_rect)),

		clip_rect: $ =>
			seq(
				'rect',
				'(',
				optional($.wsp),
				$.length_or_percentage_or_auto,
				$.comma_wsp,
				$.length_or_percentage_or_auto,
				$.comma_wsp,
				$.length_or_percentage_or_auto,
				$.comma_wsp,
				$.length_or_percentage_or_auto,
				optional($.wsp),
				')',
			),

		// ─── opacity attribute ──────────────────────────────────────

		opacity_attribute: $ =>
			seq(
				field('name', $.opacity_attribute_name),
				$._eq,
				field('value', $.opacity_attribute_value),
			),

		opacity_attribute_name: _ =>
			choice(
				'opacity',
				'fill-opacity',
				'stroke-opacity',
				'stop-opacity',
				'flood-opacity',
			),

		opacity_attribute_value: $ => quoted($.number_or_percentage),

		// ─── length attribute (x, y, width, height, etc.) ───────────

		length_attribute: $ =>
			seq(
				field('name', $.length_attribute_name),
				$._eq,
				field('value', $.length_attribute_value),
			),

		length_attribute_name: _ =>
			choice(
				'x',
				'y',
				'width',
				'height',
				'cx',
				'cy',
				'r',
				'rx',
				'ry',
				'x1',
				'y1',
				'x2',
				'y2',
				'fx',
				'fy',
				'refX',
				'refY',
				'markerWidth',
				'markerHeight',
				'stroke-width',
				'stroke-dashoffset',
				'font-size',
				'startOffset',
				'textLength',
			),

		length_attribute_value: $ => quoted($.length_or_percentage_or_auto),

		// ─── offset attribute (number or percentage, no units) ──────

		offset_attribute: $ =>
			seq(
				field('name', $.offset_attribute_name),
				$._eq,
				field('value', $.offset_attribute_value),
			),

		offset_attribute_name: _ => 'offset',

		offset_attribute_value: $ => quoted($.number_or_percentage),

		// ─── number attribute (pure numeric, no units) ──────────────

		number_attribute: $ =>
			seq(
				field('name', $.number_attribute_name),
				$._eq,
				field('value', $.number_attribute_value),
			),

		number_attribute_name: _ =>
			choice(
				'pathLength',
				'stroke-miterlimit',
				'k1',
				'k2',
				'k3',
				'k4',
				'seed',
				'scale',
				'azimuth',
				'elevation',
				'z',
				'numOctaves',
				'divisor',
				'bias',
				'surfaceScale',
				'diffuseConstant',
				'specularConstant',
				'specularExponent',
				'limitingConeAngle',
			),

		number_attribute_value: $ => quoted($.number),

		// ─── length-list attribute (dx, dy, stroke-dasharray) ───────

		length_list_attribute: $ =>
			seq(
				field('name', $.length_list_attribute_name),
				$._eq,
				field('value', $.length_list_attribute_value),
			),

		length_list_attribute_name: _ =>
			choice(
				'dx',
				'dy',
			),

		length_list_attribute_value: $ => quoted($.length_list),

		length_list: $ =>
			seq(
				$.length_or_percentage,
				repeat(seq($.comma_wsp, $.length_or_percentage)),
			),

		// ─── stroke-dasharray attribute (none or length list) ───────

		stroke_dasharray_attribute: $ =>
			seq(
				field('name', $.stroke_dasharray_attribute_name),
				$._eq,
				field('value', $.stroke_dasharray_attribute_value),
			),

		stroke_dasharray_attribute_name: _ => 'stroke-dasharray',

		stroke_dasharray_attribute_value: $ => quoted(choice('none', 'inherit', $.length_list)),

		// ─── number-list attribute (bare numbers, no units) ─────────

		number_list_attribute: $ =>
			seq(
				field('name', $.number_list_attribute_name),
				$._eq,
				field('value', $.number_list_attribute_value),
			),

		number_list_attribute_name: _ =>
			choice(
				'rotate',
				'radius',
				'stdDeviation',
				'baseFrequency',
			),

		number_list_attribute_value: $ => quoted($.number_list),

		number_list: $ =>
			seq(
				$.number,
				repeat(seq($.comma_wsp, $.number)),
			),

		// ─── duration attribute (time values) ───────────────────────

		duration_attribute: $ =>
			seq(
				field('name', $.duration_attribute_name),
				$._eq,
				field('value', $.duration_attribute_value),
			),

		duration_attribute_name: _ => choice('dur', 'repeatDur'),

		duration_attribute_value: $ => quoted(choice($.time_value, 'indefinite', 'media')),

		time_value: $ => seq($.number, optional($.time_unit)),

		// ─── repeatCount attribute ──────────────────────────────────

		repeat_count_attribute: $ =>
			seq(
				field('name', $.repeat_count_attribute_name),
				$._eq,
				field('value', $.repeat_count_attribute_value),
			),

		repeat_count_attribute_name: _ => 'repeatCount',

		repeat_count_attribute_value: $ => quoted(choice($.number, 'indefinite')),

		// ─── keyTimes attribute (semicolon-separated numbers) ───────

		key_times_attribute: $ =>
			seq(
				field('name', $.key_times_attribute_name),
				$._eq,
				field('value', $.key_times_attribute_value),
			),

		key_times_attribute_name: _ => 'keyTimes',

		key_times_attribute_value: $ => quoted($.semicolon_number_list),

		semicolon_number_list: $ =>
			seq(
				$.number,
				repeat(seq(optional($.wsp), ';', optional($.wsp), $.number)),
			),

		// ─── keySplines attribute (semicolon-separated 4-tuples) ────

		key_splines_attribute: $ =>
			seq(
				field('name', $.key_splines_attribute_name),
				$._eq,
				field('value', $.key_splines_attribute_value),
			),

		key_splines_attribute_name: _ => 'keySplines',

		key_splines_attribute_value: $ => quoted($.key_splines_list),

		key_splines_list: $ =>
			seq(
				$.key_spline_value,
				repeat(seq(optional($.wsp), ';', optional($.wsp), $.key_spline_value)),
			),

		key_spline_value: $ => seq($.number, $.comma_wsp, $.number, $.comma_wsp, $.number, $.comma_wsp, $.number),

		// ─── enable-background attribute ────────────────────────────

		enable_background_attribute: $ =>
			seq(
				field('name', $.enable_background_attribute_name),
				$._eq,
				field('value', $.enable_background_attribute_value),
			),

		enable_background_attribute_name: _ => 'enable-background',

		enable_background_attribute_value: $ => quoted(choice('accumulate', $.enable_background_new)),

		enable_background_new: $ =>
			seq(
				'new',
				optional(seq($.wsp, $.number, $.wsp, $.number, $.wsp, $.number, $.wsp, $.number)),
			),

		// ─── href attribute ─────────────────────────────────────────

		href_attribute: $ =>
			seq(
				field('name', $.href_attribute_name),
				$._eq,
				field('value', $.href_attribute_value),
			),

		href_attribute_name: _ => choice('href', 'xlink:href'),

		href_attribute_value: $ => quoted($.href_reference),

		href_reference: $ => choice($.data_uri, $.iri_reference),

		// ─── id attribute ───────────────────────────────────────────

		id_attribute: $ =>
			seq(
				field('name', $.id_attribute_name),
				$._eq,
				field('value', $.id_attribute_value),
			),

		id_attribute_name: _ => 'id',

		id_attribute_value: $ => quoted($.id_token),

		id_token: _ => token(/(?:[A-Za-z_:]|[\u0080-\uFFFF])(?:[A-Za-z0-9_.:-]|[\u0080-\uFFFF])*/),

		// ─── class attribute ────────────────────────────────────────

		class_attribute: $ =>
			seq(
				field('name', $.class_attribute_name),
				$._eq,
				field('value', $.class_attribute_value),
			),

		class_attribute_name: _ => 'class',

		class_attribute_value: $ => quoted($.class_list),

		class_list: $ => seq($.class_name, repeat(seq($.wsp, $.class_name))),

		class_name: _ => token(/[A-Za-z_][A-Za-z0-9_-]*/),

		// ─── event attribute (JS injection) ─────────────────────────

		event_attribute: $ =>
			seq(
				field('name', $.event_attribute_name),
				$._eq,
				field('value', $.event_attribute_value),
			),

		event_attribute_name: _ => token(prec(1, /on[A-Za-z][A-Za-z0-9_-]*/)),

		// Manual quoting (not `quoted()`) — each quote type needs a distinct
		// inner token (script_text_double/single) for injection targeting.
		event_attribute_value: $ =>
			choice(
				seq('"', optional(field('content', $.script_text_double)), '"'),
				seq("'", optional(field('content', $.script_text_single)), "'"),
			),

		script_text_double: _ => token(/[^"]+/),
		script_text_single: _ => token(/[^']+/),

		// ─── Generic attribute ──────────────────────────────────────

		generic_attribute: $ =>
			seq(
				field('name', $.attribute_name),
				$._eq,
				field('value', $.quoted_attribute_value),
			),

		attribute_name: _ => token(/(?:[A-Za-z_:]|[\u0080-\uFFFF])(?:[A-Za-z0-9_.:-]|[\u0080-\uFFFF])*/),

		quoted_attribute_value: $ =>
			choice(
				seq(
					'"',
					repeat(choice($.entity_reference, $.attribute_text_double)),
					'"',
				),
				seq(
					"'",
					repeat(choice($.entity_reference, $.attribute_text_single)),
					"'",
				),
			),

		attribute_text_double: _ => token(/[^"&<]+/),
		attribute_text_single: _ => token(/[^'&<]+/),

		// ─── Shared value types ─────────────────────────────────────

		_eq: $ => seq(optional($._s), '=', optional($._s)),

		data_uri: $ =>
			seq(
				'data:',
				optional(field('media_type', $.data_uri_media_type)),
				repeat(field('parameter', $.data_uri_parameter)),
				choice(field('encoding', $.data_uri_encoding), ','),
				optional(field('payload', $.data_uri_payload)),
			),

		data_uri_media_type: $ => $.mime_type,

		data_uri_parameter: $ =>
			seq(
				';',
				field('name', $.data_uri_parameter_name),
				optional(seq('=', field('value', $.data_uri_parameter_value))),
			),

		data_uri_parameter_name: _ => token(/[A-Za-z0-9!#$&^_.+-]+/),
		data_uri_parameter_value: _ => token(/[^;,"'&<]+/),
		// Consume the trailing comma so `;base64=...` and `;base64foo`
		// remain normal parameters instead of losing their `;base64` prefix
		// to a higher-precedence encoding token.
		data_uri_encoding: _ => token(/;[Bb][Aa][Ss][Ee]64,/),
		data_uri_payload: _ => token(/[^"'&<]+/),

		mime_type: _ => token(/[A-Za-z0-9!#$&^_.+-]+\/[A-Za-z0-9!#$&^_.+-]+/),

		number_or_percentage: $ => choice($.number, $.percentage),
		length_or_percentage: $ => choice($.length, $.percentage),
		length_or_percentage_or_auto: $ => choice($.length_or_percentage, 'auto'),

		length: $ => seq($.number, optional($.length_unit)),

		percentage: $ => seq($.number, '%'),

		length_unit: _ =>
			choice(
				'em',
				'ex',
				'px',
				'cm',
				'mm',
				'in',
				'pt',
				'pc',
				'Q',
				'q',
				'rem',
				'ch',
				'vh',
				'vw',
				'vmin',
				'vmax',
			),

		time_unit: _ =>
			choice(
				's',
				'ms',
				'min',
				'h',
			),

		number: _ => token(NUMBER_PATTERN),

		iri_reference: _ => token(prec(-1, /(?:#[A-Za-z_:][A-Za-z0-9_.:-]*|[^)\s"']+)/)),

		comma_wsp: $ => choice(seq(optional($.wsp), ',', optional($.wsp)), $.wsp),

		wsp: _ => token(/[ \t\r\n]+/),

		entity_reference: _ => token(/&(#x[0-9A-Fa-f]+|#[0-9]+|[A-Za-z_:][A-Za-z0-9_.:-]*);/),

		text: _ => token(prec(-1, /[^<&]+/)),

		name: _ => token(/(?:[A-Za-z_:]|[\u0080-\uFFFF])(?:[A-Za-z0-9_.:-]|[\u0080-\uFFFF])*/),

		_s: _ => token(/[ \t\r\n]+/),
	},
});
