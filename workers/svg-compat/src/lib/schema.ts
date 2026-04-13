/**
 * JSON Schema (2020-12) describing the `/data.json` response shape.
 * Served verbatim at `/schema.json` and emitted by the CLI's
 * `emit schema` subcommand.
 *
 * Kept in its own module so the schema constant can be imported
 * without dragging in any of the parse / build runtime.
 *
 * @module
 */

export const SVG_COMPAT_SCHEMA = {
	$schema: "https://json-schema.org/draft/2020-12/schema",
	title: "SVG Compat Output",
	type: "object",
	required: ["generated_at", "sources", "elements", "attributes"],
	additionalProperties: false,
	properties: {
		generated_at: { type: "string", format: "date-time" },
		sources: {
			type: "object",
			required: ["bcd", "web_features"],
			additionalProperties: false,
			properties: {
				bcd: { $ref: "#/$defs/sourceInfo" },
				web_features: { $ref: "#/$defs/sourceInfo" },
			},
		},
		elements: {
			type: "object",
			additionalProperties: { $ref: "#/$defs/compatEntry" },
		},
		attributes: {
			type: "object",
			additionalProperties: { $ref: "#/$defs/attributeEntry" },
		},
	},
	$defs: {
		sourceInfo: {
			type: "object",
			required: ["package", "requested", "resolved", "mode", "source_url"],
			additionalProperties: false,
			properties: {
				package: { type: "string" },
				requested: { type: "string" },
				resolved: { type: "string" },
				mode: { enum: ["default", "override"] },
				source_url: { type: "string", format: "uri" },
			},
		},
		baseline: {
			type: "object",
			required: ["status"],
			additionalProperties: false,
			properties: {
				status: { type: "string" },
				/* Set when upstream baseline value was unrecognised; original preserved. */
				raw_status: { type: "string" },
				since: { type: "integer" },
				since_qualifier: { enum: ["before", "after", "approximately"] },
				low_date: { $ref: "#/$defs/baselineDate" },
				high_date: { $ref: "#/$defs/baselineDate" },
			},
		},
		baselineDate: {
			type: "object",
			required: ["raw"],
			additionalProperties: false,
			properties: {
				/* Original upstream value, byte-for-byte. Always present. */
				raw: { type: "string" },
				/* ISO YYYY-MM-DD extracted from `raw`. Absent if unparseable. */
				date: { type: "string", format: "date" },
				qualifier: { enum: ["before", "after", "approximately"] },
			},
		},
		browserSupport: {
			type: "object",
			additionalProperties: false,
			properties: {
				chrome: { type: "string" },
				edge: { type: "string" },
				firefox: { type: "string" },
				safari: { type: "string" },
			},
		},
		compatEntry: {
			type: "object",
			required: ["deprecated", "experimental", "standard_track", "spec_url"],
			additionalProperties: false,
			properties: {
				description: { type: "string" },
				mdn_url: { type: "string", format: "uri" },
				deprecated: { type: "boolean" },
				experimental: { type: "boolean" },
				standard_track: { type: "boolean" },
				spec_url: {
					type: "array",
					items: { type: "string", format: "uri" },
				},
				baseline: { $ref: "#/$defs/baseline" },
				browser_support: { $ref: "#/$defs/browserSupport" },
			},
		},
		attributeEntry: {
			allOf: [
				{ $ref: "#/$defs/compatEntry" },
				{
					type: "object",
					required: ["elements"],
					properties: {
						elements: {
							type: "array",
							items: { type: "string" },
							minItems: 1,
						},
					},
				},
			],
		},
	},
} as const;
