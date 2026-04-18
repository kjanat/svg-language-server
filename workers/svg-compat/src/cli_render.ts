/**
 * Human-readable summary renderers for `svg-compat` CLI commands.
 *
 * Used in interactive TTY mode, where a full JSON dump would be hostile to read.\
 * For machine consumers (`--json`, piped stdout), `cli.ts` bypasses these and
 * emits the raw structured output.
 *
 * Renderers take a dreamcli `Out` channel and push formatted tables + log lines
 * into it.\
 * They never touch stdout/stderr directly — all routing and verbosity filtering
 * is dreamcli's job.
 *
 * @module
 */

import type { Out } from "@kjanat/dreamcli";

import type { CompatEntry, SvgCompatOutput } from "./lib/mod.ts";

type BaselineBucket = "widely" | "newly" | "limited" | "unknown";

// Index signature is required for dreamcli's
// `out.table<T extends Record<string, unknown>>` constraint — a finite-key
// interface isn't structurally assignable even though every field is `number`.\
// The named fields still document and type-check each bucket.
interface BaselineCounts {
	widely: number;
	newly: number;
	limited: number;
	unknown: number;
	total: number;
	[bucket: string]: number;
}

function bucketOf(entry: CompatEntry): BaselineBucket {
	return entry.baseline?.status ?? "unknown";
}

function countBuckets(entries: Record<string, CompatEntry>): BaselineCounts {
	const counts: BaselineCounts = {
		widely: 0,
		newly: 0,
		limited: 0,
		unknown: 0,
		total: 0,
	};
	for (const entry of Object.values(entries)) {
		counts[bucketOf(entry)] += 1;
		counts.total += 1;
	}
	return counts;
}

/**
 * Renders a human-readable summary of the `/data.json` payload:
 * header, sources table, element baseline breakdown, attribute
 * baseline breakdown, and a footer hint for full JSON output.
 */
export function renderDataSummary(out: Out, data: SvgCompatOutput): void {
	out.log(`svg-compat · generated ${data.generated_at}`);
	out.log("");

	out.log("sources");
	out.table([
		{
			role: "bcd",
			package: data.sources.bcd.package,
			resolved: data.sources.bcd.resolved,
			mode: data.sources.bcd.mode,
		},
		{
			role: "web-features",
			package: data.sources.web_features.package,
			resolved: data.sources.web_features.resolved,
			mode: data.sources.web_features.mode,
		},
	]);
	out.log("");

	out.log("elements (baseline buckets)");
	out.table([countBuckets(data.elements)]);
	out.log("");

	out.log("attributes (baseline buckets)");
	out.table([countBuckets(data.attributes)]);
	out.log("");

	out.log("(pass --json or pipe stdout for the full structured dump)");
}

interface SchemaShape {
	readonly title?: unknown;
	readonly $schema?: unknown;
	readonly required?: unknown;
	readonly properties?: unknown;
}

function asString(value: unknown): string | undefined {
	return typeof value === "string" ? value : undefined;
}

function asStringArray(value: unknown): readonly string[] {
	return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

function propertyNames(value: unknown): readonly string[] {
	return value !== null && typeof value === "object" ? Object.keys(value as object) : [];
}

/**
 * Renders a compact summary of the JSON Schema: title, draft URL,
 * required top-level fields, and full list of top-level properties.
 * Leaves the full `$defs` walk to the JSON dump (users who need
 * every detail should pipe stdout or pass `--json`).
 */
export function renderSchemaSummary(out: Out, schema: unknown): void {
	const shape = (schema ?? {}) as SchemaShape;
	const title = asString(shape.title) ?? "(untitled schema)";
	const draft = asString(shape.$schema) ?? "(no $schema)";
	const required = asStringArray(shape.required);
	const props = propertyNames(shape.properties);

	out.log(`svg-compat schema · ${title}`);
	out.log(`draft: ${draft}`);
	out.log("");

	if (props.length > 0) {
		out.log("top-level properties");
		out.table(
			props.map((name) => ({
				name,
				required: required.includes(name) ? "yes" : "no",
			})),
		);
		out.log("");
	}

	out.log("(pass --json or pipe stdout for the full schema dump)");
}
