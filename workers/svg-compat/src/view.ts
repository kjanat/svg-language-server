/**
 * View-model builders for the HTML dashboard.
 *
 * Pure data — no JSX, no Preact. Converts {@linkcode SvgCompatOutput}
 * into a shape the Preact components consume directly, and pre-computes
 * lowercase search-token strings baked into `data-search` attributes.
 *
 * @module
 */

import type { AttributeEntry, BrowserSupport, BrowserVersion, CompatEntry, SvgCompatOutput } from "./main.ts";
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
	/** Total element count. */
	elements: number;
	/** Total attribute count. */
	attributes: number;
	/** Count of deprecated elements (BCD `deprecated: true`). */
	deprecated: number;
	/** Count of attributes with `baseline: "limited"`. */
	limited: number;
	/** Count of entries where at least one browser has `partial_implementation`. */
	partial: number;
	/** Count of entries where at least one browser carries a `version_removed` value. */
	removed: number;
	/** Count of entries with any `flags` gating the feature in a browser. */
	flagged: number;
	/** Count of entries with at least one explicit `supported: false`. */
	unsupportedSomewhere: number;
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
const QUALIFIER_GLYPH: Record<NonNullable<BrowserVersion["version_qualifier"]>, string> = {
	before: "≤",
	after: "≥",
	approximately: "~",
};

/**
 * Short text shown inside the chip for one browser. Mirrors what
 * {@link BrowserSupport} (the component) renders so the column-width
 * computation stays in sync with the actual glyph layout.
 */
export function browserVersionChipLabel(version: BrowserVersion): string {
	if (version.supported === false) return "—";
	if (version.version_added !== undefined) {
		const glyph = version.version_qualifier
			? QUALIFIER_GLYPH[version.version_qualifier]
			: "";
		return `${glyph}${version.version_added}`;
	}
	if (version.raw_value_added === true) return "✓";
	return "—";
}

function computeBrowserMaxChars(elements: NamedCompatEntry[]): BrowserMaxChars {
	const max: BrowserMaxChars = { chrome: 1, edge: 1, firefox: 1, safari: 1 };
	for (const entry of elements) {
		const support = entry.browser_support;
		if (!support) continue;
		for (const key of BROWSER_KEYS) {
			const version = support[key];
			if (version === undefined) continue;
			const label = browserVersionChipLabel(version);
			if (label.length > max[key]) max[key] = label.length;
		}
	}
	return max;
}

/**
 * Builds a dashboard {@linkcode PageModel} from the processed output and
 * request URL. Preserves the bcd→web_features iteration order so the
 * Upstream Sources table matches the legacy render.
 */
/**
 * Single pass over elements + attributes counting per-entry signal
 * presence. An entry contributes to a counter once if *any* browser
 * carries the signal — we're measuring the blast radius of the
 * underlying upstream fact, not the total number of per-browser
 * occurrences.
 */
function countBrowserSignals(
	entries: CompatEntry[],
): {
	partial: number;
	removed: number;
	flagged: number;
	unsupportedSomewhere: number;
} {
	let partial = 0;
	let removed = 0;
	let flagged = 0;
	let unsupportedSomewhere = 0;
	for (const entry of entries) {
		const support = entry.browser_support;
		if (!support) continue;
		let hasPartial = false;
		let hasRemoved = false;
		let hasFlagged = false;
		let hasUnsupported = false;
		for (const key of BROWSER_KEYS) {
			const v = support[key];
			if (v === undefined) continue;
			if (v.partial_implementation) hasPartial = true;
			if (v.version_removed !== undefined) hasRemoved = true;
			if (v.flags !== undefined) hasFlagged = true;
			if (v.supported === false) hasUnsupported = true;
		}
		if (hasPartial) partial++;
		if (hasRemoved) removed++;
		if (hasFlagged) flagged++;
		if (hasUnsupported) unsupportedSomewhere++;
	}
	return { partial, removed, flagged, unsupportedSomewhere };
}

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
	const jsonUrl = new URL("/data.json", requestUrl);
	// Keep the exact active source selection in the JSON endpoint link so
	// the exported payload matches the versions currently shown in the UI.
	jsonUrl.search = requestUrl.search;
	const signalCounts = countBrowserSignals([...elements, ...attributes]);

	return {
		generatedAt: output.generated_at,
		stats: {
			elements: elements.length,
			attributes: attributes.length,
			deprecated: deprecatedElements.length,
			limited: limitedAttributes.length,
			partial: signalCounts.partial,
			removed: signalCounts.removed,
			flagged: signalCounts.flagged,
			unsupportedSomewhere: signalCounts.unsupportedSomewhere,
		},
		sources: Object.values(output.sources),
		elements,
		attributes,
		deprecatedElements,
		limitedAttributes,
		urls: {
			json: jsonUrl.toString(),
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
	const tokens: string[] = [];
	let anySupported = false;
	let anyUnsupported = false;
	let anyRemoved = false;
	let anyPartial = false;
	let anyPrefixed = false;
	let anyFlagged = false;
	for (const key of BROWSER_KEYS) {
		const version = support[key];
		if (version === undefined) continue;
		tokens.push(key);
		if (version.supported === false) anyUnsupported = true;
		else if (version.version_added !== undefined || version.raw_value_added === true) {
			anySupported = true;
		}
		if (version.version_removed !== undefined) anyRemoved = true;
		if (version.partial_implementation) anyPartial = true;
		if (version.prefix !== undefined) anyPrefixed = true;
		if (version.flags !== undefined) anyFlagged = true;
	}
	if (anySupported) tokens.push("supported");
	if (anyUnsupported) tokens.push("unsupported");
	if (anyRemoved) tokens.push("removed");
	if (anyPartial) tokens.push("partial");
	if (anyPrefixed) tokens.push("prefixed");
	if (anyFlagged) tokens.push("flagged");
	return tokens;
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
