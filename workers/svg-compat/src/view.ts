/**
 * View-model builders for the HTML dashboard.
 *
 * Pure data — no JSX, no Preact. Converts {@linkcode SvgCompatOutput}
 * into a shape the Preact components consume directly, and pre-computes
 * lowercase search-token strings baked into `data-search` attributes.
 *
 * @module
 */

import type { AttributeEntry, BrowserSupport, CompatEntry, SvgCompatOutput } from "./main.ts";
import type { SourceInfo } from "./sources.ts";

/** Browser keys used in chip rendering; ordered for display. */
export const BROWSER_KEYS = [
	"chrome",
	"edge",
	"firefox",
	"safari",
] as const satisfies readonly (keyof BrowserSupport)[];

/** Maximum version-string character count per browser across all elements. */
export type BrowserMaxChars = Record<(typeof BROWSER_KEYS)[number], number>;

/** Compat entry + the tag name it belongs to (keyed from the output record). */
export interface NamedCompatEntry extends CompatEntry {
	/** Element tag name (e.g. `"circle"`). */
	name: string;
}

/** Attribute entry + the attribute name it belongs to. */
export interface NamedAttributeEntry extends AttributeEntry {
	/** Attribute name (e.g. `"fill"`). */
	name: string;
}

/** Count of each data bucket shown in the hero stats grid. */
export interface PageStats {
	elements: number;
	attributes: number;
	deprecated: number;
	limited: number;
}

/** URLs surfaced in the hero pill buttons. */
export interface PageUrls {
	json: string;
	schema: string;
	latestHtml: string;
	latestJson: string;
}

/** Everything the dashboard components need to render. */
export interface PageModel {
	generatedAt: string;
	stats: PageStats;
	sources: SourceInfo[];
	elements: NamedCompatEntry[];
	attributes: NamedAttributeEntry[];
	deprecatedElements: NamedCompatEntry[];
	limitedAttributes: NamedAttributeEntry[];
	urls: PageUrls;
	browserMaxChars: BrowserMaxChars;
}

function named<T>(record: Record<string, T>): Array<T & { name: string }> {
	return Object.entries(record).map(([name, entry]) => ({ name, ...entry }));
}

/**
 * Computes the longest version-string length for each browser across
 * all element entries. Drives per-browser chip column widths so they
 * adapt to the current dataset (e.g. a future Chrome "100" would widen
 * the Chrome column from 2 to 3 chars automatically).
 *
 * Elements without recorded support for a browser don't contribute —
 * the missing-state chip shows "—" which always fits in 1 char.
 *
 * @param elements Named element entries from the page model.
 * @returns Max character count per browser key. Floors at 1 so empty
 *   datasets still produce a sane layout.
 */
function computeBrowserMaxChars(elements: NamedCompatEntry[]): BrowserMaxChars {
	const max: BrowserMaxChars = { chrome: 1, edge: 1, firefox: 1, safari: 1 };
	for (const entry of elements) {
		const support = entry.browser_support;
		if (!support) continue;
		for (const key of BROWSER_KEYS) {
			const version = support[key];
			if (version !== undefined && version.length > max[key]) {
				max[key] = version.length;
			}
		}
	}
	return max;
}

/**
 * Builds a dashboard {@linkcode PageModel} from the processed output and
 * request URL. Preserves the bcd→web_features iteration order so the
 * Upstream Sources table matches the legacy render.
 */
export function buildPageModel(
	output: SvgCompatOutput,
	requestUrl: URL,
): PageModel {
	const elements = named(output.elements);
	const attributes = named(output.attributes);
	const deprecatedElements = elements.filter((entry) => entry.deprecated);
	const limitedAttributes = attributes.filter(
		(entry) => entry.baseline?.status === "limited",
	);
	const origin = requestUrl.origin;

	return {
		generatedAt: output.generated_at,
		stats: {
			elements: elements.length,
			attributes: attributes.length,
			deprecated: deprecatedElements.length,
			limited: limitedAttributes.length,
		},
		sources: Object.values(output.sources),
		elements,
		attributes,
		deprecatedElements,
		limitedAttributes,
		urls: {
			json: `${origin}/data.json`,
			schema: `${origin}/schema.json`,
			latestHtml: `${origin}/?source=latest`,
			latestJson: `${origin}/data.json?source=latest`,
		},
		browserMaxChars: computeBrowserMaxChars(elements),
	};
}

function browserTokens(entry: CompatEntry): string[] {
	const support = entry.browser_support;
	if (!support) return [];
	const names: string[] = [];
	if (support.chrome !== undefined) names.push("chrome");
	if (support.edge !== undefined) names.push("edge");
	if (support.firefox !== undefined) names.push("firefox");
	if (support.safari !== undefined) names.push("safari");
	return names;
}

function baselineTokens(entry: CompatEntry): string[] {
	const baseline = entry.baseline;
	if (!baseline) return [];
	const tokens: string[] = [baseline.status];
	if (baseline.since !== undefined) tokens.push(String(baseline.since));
	return tokens;
}

function normaliseToken(value: string): string {
	return value.toLowerCase();
}

function joinTokens(parts: string[]): string {
	return parts.map(normaliseToken).filter((part) => part.length > 0).join(" ");
}

/** Pre-lowered search tokens for an element row. */
export function elementSearchTokens(entry: NamedCompatEntry): string {
	return joinTokens([
		entry.name,
		...baselineTokens(entry),
		...browserTokens(entry),
		entry.deprecated ? "deprecated" : "",
	]);
}

/** Pre-lowered search tokens for an attribute row. */
export function attributeSearchTokens(entry: NamedAttributeEntry): string {
	const scope = entry.elements.length === 1 && entry.elements[0] === "*"
		? ["global"]
		: entry.elements;
	return joinTokens([
		entry.name,
		...scope,
		...baselineTokens(entry),
		...browserTokens(entry),
	]);
}

/** Pre-lowered search tokens for an upstream-source row. */
export function sourceSearchTokens(source: SourceInfo): string {
	return joinTokens([
		source.package,
		source.requested,
		source.resolved,
		source.mode,
	]);
}
