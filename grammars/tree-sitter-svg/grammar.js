/**
 * @file SVG grammar for Tree-sitter
 * @author Kaj Kowalski <info@kajkowalski.nl>
 * @license MIT
 */
/// <reference types="tree-sitter-cli/dsl" />

import grammarData from '#grammarData' with { type: 'json' };
import { D_ATTRIBUTE_NAMES } from '#grammarFixtures';

const NUMBER_PATTERN = /[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?/;

// Literal delimiter tokens, named so the rules read intent over punctuation.
const DQUOTE = '"';
const SQUOTE = "'";
const LANGLE = '<';
const RANGLE = '>';
const LPAREN = '(';
const RPAREN = ')';
const COMMA = ',';
const SEMI = ';';
const EQ = '=';
const ATTRIBUTE_BUCKETS = grammarData.attribute_buckets;
const TOKENS = grammarData.tokens;

/** @param {RuleOrLiteral} value */
function quoted(value) {
	return choice(
		seq(DQUOTE, optional(value), DQUOTE),
		seq(SQUOTE, optional(value), SQUOTE),
	);
}

// Data-driven alternation over a catalog-derived list (attribute spellings,
// unit tokens, etc.). Wrap the bucket in one token so tree-sitter emits one
// compact lexer branch instead of one token per spelling. A one-element bucket
// (e.g. `view_box: ['viewBox']`) still avoids `choice(x)`, which tree-sitter
// flags as unnecessary.
/** @param {readonly RuleOrLiteral[]} members */
function oneOf(members) {
	if (members.length === 0) {
		return token(/\x00/);
	}
	const [first, ...rest] = members;
	return token(rest.length === 0 ? first : choice(first, ...rest));
}

export default grammar({
	name: 'svg',

	externals: $ => [
		$._start_tag_name,
		$._path_start_tag_name,
		$._animate_motion_start_tag_name,
		$._script_start_tag_name,
		$._style_start_tag_name,
		$._end_tag_name,
		$._path_end_tag_name,
		$._animate_motion_end_tag_name,
		$._script_end_tag_name,
		$._style_end_tag_name,
		$._erroneous_end_tag_name,
		$._raw_text,
		'/>',
		$._cdata_text,
	],

	extras: () => [],

	// A lone number (`values="0"`) is a list-of-one shared by the whitespace/
	// comma-separated `number_list` (filter context) and the `;`-separated
	// `semicolon_number_list` (SMIL animation context). Both are valid for the
	// `values` attribute; the GLR parser keeps both stacks until the first
	// separator selects the arm.
	conflicts: $ => [
		[$.number_list, $.semicolon_number_list],
	],

	// Each rule here was a visible `choice` wrapper over single named symbols;
	// promoting it to a supertype makes that wrapper node TRANSPARENT in the
	// CST (the concrete subtype appears directly, no extra level) while
	// node-types.json links it via a `subtypes` array. Consumers query the
	// supertype name to match all kinds in one capture — e.g. `(attribute)`,
	// `(path_segment)` — or switch on the concrete subtype. Because the wrapper
	// level is removed, queries that nested a subtype under the supertype
	// (`(attribute (id_attribute ...))`) must drop that level
	// (`(id_attribute ...)`), and corpus expectations lose the wrapper node.
	supertypes: $ => [
		$.attribute,
	],

	// Hidden single-use wrapper rules substituted at their (one) use site. Each
	// is referenced exactly once and bears no `field()`, so inlining removes a
	// hidden node-type and a few LR states with no visible-tree change. Limited
	// to single-use rules ON PURPOSE: inlining the multi-use color components
	// (`_color_component` ×15, `_color_alpha` ×8, `_color_hue_component` ×3)
	// DUPLICATES their choice bodies across all nine color functions and
	// EXPLODES the state count (~+850 states, measured) — the opposite of the
	// goal — so they stay as hidden shared rules. `_length_or_keyword_item`
	// (×2) is state-neutral but a thin dispatcher, kept for node-type clarity.
	inline: $ => [
		$._length_or_keyword_item,
	],

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
		xml_standalone_attribute_value: _ => token(choice('"yes"', '"no"', "'yes'", "'no'")),

		doctype: $ =>
			seq(
				'<!DOCTYPE',
				$._s,
				field('name', $.name),
				optional(seq($._s, field('external_id', $.doctype_external_id))),
				optional(seq($._s, field('internal_subset', $.doctype_internal_subset))),
				optional($._s),
				RANGLE,
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
				$._animate_motion_element,
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
					optional($._raw_or_cdata_text),
					choice(alias($.script_end_tag, $.end_tag), $.erroneous_end_tag),
				),
			),

		script_start_tag: $ =>
			seq(
				LANGLE,
				field('name', alias($._script_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				RANGLE,
			),

		script_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._script_end_tag_name, $.name)),
				optional($._s),
				RANGLE,
			),

		script_self_closing_tag: $ =>
			seq(
				LANGLE,
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
					optional($._raw_or_cdata_text),
					choice(alias($.style_end_tag, $.end_tag), $.erroneous_end_tag),
				),
			),

		_raw_or_cdata_text: $ =>
			choice(
				seq(
					optional(alias($._raw_text, $.raw_text)),
					$.cdata_section,
					optional(alias($._raw_text, $.raw_text)),
				),
				alias($._raw_text, $.raw_text),
			),

		style_element_start_tag: $ =>
			seq(
				LANGLE,
				field('name', alias($._style_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				RANGLE,
			),

		style_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._style_end_tag_name, $.name)),
				optional($._s),
				RANGLE,
			),

		style_self_closing_tag: $ =>
			seq(
				LANGLE,
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
				LANGLE,
				field('name', alias($._path_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				RANGLE,
			),

		path_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._path_end_tag_name, $.name)),
				optional($._s),
				RANGLE,
			),

		path_self_closing_tag: $ =>
			seq(
				LANGLE,
				field('name', alias($._path_start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				'/>',
			),

		// ─── animateMotion Element (motion coordinate attrs) ───────────

		_animate_motion_element: $ =>
			choice(
				alias($.animate_motion_self_closing_tag, $.self_closing_tag),
				seq(
					alias($.animate_motion_start_tag, $.start_tag),
					repeat($._content),
					choice(alias($.animate_motion_end_tag, $.end_tag), $.erroneous_end_tag),
				),
			),

		animate_motion_start_tag: $ =>
			seq(
				LANGLE,
				field('name', alias($._animate_motion_start_tag_name, $.name)),
				repeat(seq($._s, $.animate_motion_attribute)),
				optional($._s),
				RANGLE,
			),

		animate_motion_end_tag: $ =>
			seq(
				'</',
				field('name', alias($._animate_motion_end_tag_name, $.name)),
				optional($._s),
				RANGLE,
			),

		animate_motion_self_closing_tag: $ =>
			seq(
				LANGLE,
				field('name', alias($._animate_motion_start_tag_name, $.name)),
				repeat(seq($._s, $.animate_motion_attribute)),
				optional($._s),
				'/>',
			),

		animate_motion_attribute: $ =>
			choice(
				$.animate_motion_coordinate_attribute,
				$.animate_motion_values_attribute,
				$.attribute,
			),

		animate_motion_coordinate_attribute: $ =>
			seq(
				field('name', $.animate_motion_coordinate_attribute_name),
				$._eq,
				field('value', $.animate_motion_coordinate_attribute_value),
			),

		animate_motion_coordinate_attribute_name: _ => token(choice('by', 'from', 'to')),

		animate_motion_coordinate_attribute_value: $ => quoted($.coordinate_pair),

		animate_motion_values_attribute: $ =>
			seq(
				field('name', $.animate_motion_values_attribute_name),
				$._eq,
				field('value', $.animate_motion_values_attribute_value),
			),

		animate_motion_values_attribute_name: _ => 'values',

		animate_motion_values_attribute_value: $ => quoted($.semicolon_coordinate_pair_list),

		// ─── Generic Tags ───────────────────────────────────────────

		start_tag: $ =>
			seq(
				LANGLE,
				field('name', alias($._start_tag_name, $.name)),
				repeat(seq($._s, $.attribute)),
				optional($._s),
				RANGLE,
			),

		end_tag: $ =>
			seq(
				'</',
				field('name', alias($._end_tag_name, $.name)),
				optional($._s),
				RANGLE,
			),

		self_closing_tag: $ =>
			seq(
				LANGLE,
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
				RANGLE,
			),

		// ─── Attributes ─────────────────────────────────────────────

		// Supertype over every concrete attribute bucket. The members are a
		// flat choice of single named symbols (28 typed + generic) so the
		// tree-sitter supertype validator accepts it; the former hidden
		// `_typed_attribute` intermediate produced zero CST nodes, so
		// flattening it here is tree-identical.
		attribute: $ =>
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
				$.number_optional_number_attribute,
				$.length_list_attribute,
				$.rotate_attribute,
				$.stroke_dasharray_attribute,
				$.css_text_attribute,
				$.keyword_attribute,
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
				$.generic_attribute,
			),

		// ─── d attribute (path data sub-grammar) ────────────────────

		d_attribute: $ =>
			seq(
				field('name', $.d_attribute_name),
				$._eq,
				field('value', $.d_attribute_value),
			),

		d_attribute_name: _ => oneOf(D_ATTRIBUTE_NAMES),

		d_attribute_value: $ =>
			choice(
				$.double_quoted_path_data,
				$.single_quoted_path_data,
			),

		double_quoted_path_data: $ =>
			seq(
				DQUOTE,
				optional(field('path', $.path_data_payload)),
				DQUOTE,
			),

		single_quoted_path_data: $ =>
			seq(
				SQUOTE,
				optional(field('path', $.path_data_payload)),
				SQUOTE,
			),

		// Opaque path-data capture. The rich path sub-grammar (commands,
		// coordinates, arcs) is evicted to a sibling `tree-sitter-svg-path`
		// grammar injected over this token's range via injections.scm — same
		// pattern as CSS-in-`<style>`. Excludes only the quote delimiters and
		// XML-significant `<`/`&`, mirroring `data_uri_payload`.
		path_data_payload: _ => token(/[^"'<&]+/),

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
				DQUOTE,
				optional(field('content', $.style_text_double)),
				DQUOTE,
			),

		single_quoted_style_value: $ =>
			seq(
				SQUOTE,
				optional(field('content', $.style_text_single)),
				SQUOTE,
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

		viewbox_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.view_box),

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
			token(choice(
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
			)),

		meet_or_slice_keyword: _ => token(choice('meet', 'slice')),

		// ─── transform attribute ────────────────────────────────────

		transform_attribute: $ =>
			seq(
				field('name', $.transform_attribute_name),
				$._eq,
				field('value', $.transform_attribute_value),
			),

		transform_attribute_name: _ =>
			token(choice(
				'transform',
				'gradientTransform',
				'patternTransform',
			)),

		// Opaque transform-list capture. The transform-function sub-grammar is
		// evicted to the sibling tree-sitter-svg-transform grammar, injected over
		// this token's range via injections.scm — same pattern as path data.
		transform_attribute_value: $ => quoted($.transform_payload),

		transform_payload: _ => token(/[^"'<&]+/),

		// ─── points attribute ───────────────────────────────────────

		points_attribute: $ =>
			seq(
				field('name', $.points_attribute_name),
				$._eq,
				field('value', $.points_attribute_value),
			),

		points_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.coordinate_pair_list),

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

		paint_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.color),

		// Opaque paint/color capture. The paint + color-value sub-grammar is
		// evicted to the sibling tree-sitter-svg-paint grammar, injected over this
		// token's range via injections.scm — same pattern as path/transform.
		paint_attribute_value: $ => quoted($.paint_payload),

		paint_payload: _ => token(/[^"'<&]+/),

		// ─── functional IRI attribute (url(#ref)) ───────────────────

		functional_iri_attribute: $ =>
			seq(
				field('name', $.functional_iri_attribute_name),
				$._eq,
				field('value', $.functional_iri_attribute_value),
			),

		functional_iri_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.functional_iri),

		functional_iri_attribute_value: $ => quoted(choice('none', $.functional_iri, $.iri_reference)),

		functional_iri: $ => seq('url(', optional($.wsp), $.iri_reference, optional($.wsp), RPAREN),

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
				LPAREN,
				optional($.wsp),
				$.length_or_percentage_or_auto,
				$.comma_wsp,
				$.length_or_percentage_or_auto,
				$.comma_wsp,
				$.length_or_percentage_or_auto,
				$.comma_wsp,
				$.length_or_percentage_or_auto,
				optional($.wsp),
				RPAREN,
			),

		// ─── opacity attribute ──────────────────────────────────────

		opacity_attribute: $ =>
			seq(
				field('name', $.opacity_attribute_name),
				$._eq,
				field('value', $.opacity_attribute_value),
			),

		opacity_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.number_or_percentage),

		opacity_attribute_value: $ => quoted($.number_or_percentage),

		// ─── length attribute (x, y, width, height, etc.) ───────────

		length_attribute: $ =>
			seq(
				field('name', $.length_attribute_name),
				$._eq,
				field('value', $.length_attribute_value),
			),

		length_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.length),

		// The `length` bucket holds attributes whose value space is a length or
		// percentage, but many spec grammars are a union with keyword alternatives
		// or keyword/length combinations: `baseline-shift` (`sub | super |
		// <length-percentage>`), `letter-spacing`/`word-spacing` (`normal |
		// <length>`), `font-size-adjust` (`none | <number>`), `refX`/`refY`
		// (`<length> | left | center | …`), and `transform-origin`
		// (`[ left | center | … | <length-percentage> ]…`). The plain
		// length/percentage/auto value keeps its existing shape; a keyword-bearing
		// value (one or more whitespace-separated keywords, optionally mixed with
		// lengths) is the alternative. A genuine pure-length attribute (`x`,
		// `width`) never carries a keyword in valid SVG, so this superset is
		// lossless and keeps the value typed.
		length_attribute_value: $ => quoted(choice($.length_or_percentage_or_auto, $.length_keyword_value)),

		// Distinct from the plain length/percentage/auto branch: this fires only
		// when a keyword participates, so a bare single length never matches both
		// branches. Either it leads with a keyword, or it leads with a length that
		// is followed by at least one more whitespace-separated item.
		length_keyword_value: $ =>
			choice(
				seq($.keyword_value, repeat(seq($.wsp, $._length_or_keyword_item))),
				seq($.length_or_percentage, repeat1(seq($.wsp, $._length_or_keyword_item))),
			),

		_length_or_keyword_item: $ => choice($.length_or_percentage, $.keyword_value),

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

		number_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.number),

		// A bare single `<number>`. The spec's number *lists* (`kernelMatrix`,
		// feColorMatrix `values`, `tableValues`) now carry their list shape
		// through the catalog (`<number>+`) and route to `number_list_attribute`,
		// so this bucket holds only genuine scalar attributes (`bias`, `divisor`,
		// `seed`, …) and needs no separated tail.
		number_attribute_value: $ => quoted($.number),

		// ── number-optional-number attribute (one or two numbers) ──
		// SVG <number-optional-number>: a single number, optionally
		// followed by whitespace and a second number (e.g. stdDeviation,
		// baseFrequency, kernelUnitLength, order, radius).

		number_optional_number_attribute: $ =>
			seq(
				field('name', $.number_optional_number_attribute_name),
				$._eq,
				field('value', $.number_optional_number_attribute_value),
			),

		number_optional_number_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.number_optional_number),

		number_optional_number_attribute_value: $ => quoted($.number_optional_number),

		number_optional_number: $ => seq($.number, optional(seq($.wsp, $.number))),

		// ─── length-list attribute (dx, dy, stroke-dasharray) ───────

		length_list_attribute: $ =>
			seq(
				field('name', $.length_list_attribute_name),
				$._eq,
				field('value', $.length_list_attribute_value),
			),

		length_list_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.length_list),

		length_list_attribute_value: $ => quoted($.length_list),

		length_list: $ =>
			seq(
				$.length_or_percentage,
				repeat(seq($.comma_wsp, $.length_or_percentage)),
			),

		// ─── rotate attribute ────────────────────────────────────────

		rotate_attribute: $ =>
			seq(
				field('name', $.rotate_attribute_name),
				$._eq,
				field('value', $.rotate_attribute_value),
			),

		rotate_attribute_name: _ => 'rotate',

		// `rotate` is scoped: text/tspan accept number lists, animateMotion accepts
		// one numeric angle or `auto`/`auto-reverse`. The grammar is not
		// element-aware, so parse the valid union under one dedicated node.
		rotate_attribute_value: $ => quoted(choice($.number_list, 'auto', 'auto-reverse')),

		// ─── stroke-dasharray attribute (none or length list) ───────

		stroke_dasharray_attribute: $ =>
			seq(
				field('name', $.stroke_dasharray_attribute_name),
				$._eq,
				field('value', $.stroke_dasharray_attribute_value),
			),

		stroke_dasharray_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.length_list_or_none),

		stroke_dasharray_attribute_value: $ => quoted(choice('none', 'inherit', $.length_list)),

		// ─── keyword-valued presentation attributes ─────────────────

		keyword_attribute: $ =>
			seq(
				field('name', $.keyword_attribute_name),
				$._eq,
				field('value', $.keyword_attribute_value),
			),

		keyword_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.keyword),

		// Most keyword attributes carry a single enum value, but CSS `||`
		// combinators (e.g. `paint-order="stroke fill markers"`) accept several
		// whitespace-separated keywords in any order. Allow a one-or-more
		// keyword list so both shapes parse; a single keyword is the list of one.
		keyword_attribute_value: $ => quoted(choice($.number, seq($.keyword_value, repeat(seq($.wsp, $.keyword_value))))),

		keyword_value: _ => token(/[A-Za-z_][A-Za-z0-9_-]*/),

		// ─── CSS-text presentation attributes ───────────────────────

		css_text_attribute: $ =>
			seq(
				field('name', $.css_text_attribute_name),
				$._eq,
				field('value', $.css_text_attribute_value),
			),

		css_text_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.css_text),

		css_text_attribute_value: $ =>
			choice(
				seq(DQUOTE, optional(field('content', $.css_attribute_text_double)), DQUOTE),
				seq(SQUOTE, optional(field('content', $.css_attribute_text_single)), SQUOTE),
			),

		css_attribute_text_double: _ => token(/[^"]+/),
		css_attribute_text_single: _ => token(/[^']+/),

		// ─── number-list attribute (bare numbers, no units) ─────────

		number_list_attribute: $ =>
			seq(
				field('name', $.number_list_attribute_name),
				$._eq,
				field('value', $.number_list_attribute_value),
			),

		number_list_attribute_name: _ => oneOf(ATTRIBUTE_BUCKETS.number_list),

		// A number list separates with whitespace/commas in the filter context
		// (`kernelMatrix`, feColorMatrix `values`, `tableValues`) but with `;`
		// when the same `values` attribute drives SMIL animation
		// (`values="0;10;20"`). Motion coordinate-pair lists are scoped to
		// `animateMotion` so generic number-list attrs cannot accept coordinates.
		number_list_attribute_value: $ =>
			quoted(choice(
				$.number_list,
				$.semicolon_number_list,
			)),

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

		duration_attribute_name: _ => token(choice('dur', 'repeatDur')),

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
				repeat(seq(optional($.wsp), SEMI, optional($.wsp), $.number)),
				optional(seq(optional($.wsp), SEMI, optional($.wsp))),
			),

		semicolon_coordinate_pair_list: $ =>
			seq(
				$.coordinate_pair,
				repeat(seq(optional($.wsp), SEMI, optional($.wsp), $.coordinate_pair)),
				optional(seq(optional($.wsp), SEMI, optional($.wsp))),
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
				repeat(seq(optional($.wsp), SEMI, optional($.wsp), $.key_spline_value)),
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

		href_attribute_name: _ => token(choice('href', 'xlink:href')),

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
				seq(DQUOTE, optional(field('content', $.script_text_double)), DQUOTE),
				seq(SQUOTE, optional(field('content', $.script_text_single)), SQUOTE),
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
					DQUOTE,
					repeat(choice($.entity_reference, $.attribute_text_double)),
					DQUOTE,
				),
				seq(
					SQUOTE,
					repeat(choice($.entity_reference, $.attribute_text_single)),
					SQUOTE,
				),
			),

		attribute_text_double: _ => token(/[^"&<]+/),
		attribute_text_single: _ => token(/[^'&<]+/),

		// ─── Shared value types ─────────────────────────────────────

		_eq: $ => seq(optional($._s), EQ, optional($._s)),

		data_uri: $ =>
			seq(
				'data:',
				optional(field('media_type', $.data_uri_media_type)),
				repeat(field('parameter', $.data_uri_parameter)),
				choice(field('encoding', $.data_uri_encoding), COMMA),
				optional(field('payload', $.data_uri_payload)),
			),

		data_uri_media_type: $ => $.mime_type,

		data_uri_parameter: $ =>
			seq(
				SEMI,
				field('name', $.data_uri_parameter_name),
				optional(seq(EQ, field('value', $.data_uri_parameter_value))),
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

		length_unit: _ => token(choice(...TOKENS.length_units)),

		time_unit: _ => token(choice(...TOKENS.time_units)),

		number: _ => token(NUMBER_PATTERN),

		iri_reference: _ => token(prec(-1, /(?:#[A-Za-z_:][A-Za-z0-9_.:-]*|[^)\s"']+)/)),

		comma_wsp: $ => choice(seq(optional($.wsp), COMMA, optional($.wsp)), $.wsp),

		wsp: _ => token(/[ \t\r\n]+/),

		entity_reference: _ => token(/&(#x[0-9A-Fa-f]+|#[0-9]+|[A-Za-z_:][A-Za-z0-9_.:-]*);/),

		text: _ => token(prec(-1, /[^<&]+/)),

		name: _ => token(/(?:[A-Za-z_:]|[\u0080-\uFFFF])(?:[A-Za-z0-9_.:-]|[\u0080-\uFFFF])*/),

		_s: _ => token(/[ \t\r\n]+/),
	},
});
