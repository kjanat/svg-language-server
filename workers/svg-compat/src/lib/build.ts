/**
 * Assembly logic that turns loaded BCD + web-features payloads into
 * the final `SvgCompatOutput`. Walks `bcd.svg.elements` and
 * `bcd.svg.global_attributes`, merges per-element attribute compat
 * across elements, and produces a sorted record keyed by name.
 *
 * Pure data — no Deno HTTP plumbing, no preact. Safe to call from
 * both the worker server and the CLI.
 *
 * @module
 */

import { isRecord, type JsonRecord, type LoadedSourceData, UpstreamSourceError } from "../sources.ts";
import { getCompat, getRecordProperty, makeCompatEntry } from "./parse.ts";
import type {
	AttributeEntry,
	Baseline,
	BrowserSupport,
	CompatEntry,
	SvgCompatOutput,
	SvgCompatSnapshot,
} from "./types.ts";

/**
 * BCD uses underscore-delimited namespace names (`xlink_href`,
 * `xml_lang`); SVG uses colon-delimited (`xlink:href`, `xml:lang`).
 * Canonicalise here. Regex subsumes the old hand-written XLINK_MAP
 * and auto-covers any future `xml_*` / `xlink_*` additions upstream.
 */
const NAMESPACE_UNDERSCORE = /^(xlink|xml)_(\w+)$/;

function canonicalAttributeName(name: string): string {
	const match = name.match(NAMESPACE_UNDERSCORE);
	return match ? `${match[1]}:${match[2]}` : name;
}

function baselineRank(baseline: Baseline): number {
	if (baseline.status === "limited") return 0;
	if (baseline.status === "newly") return 1;
	return 2;
}

function parseBrowserVersion(
	version: string,
): { upperBound: boolean; parts: number[] } | undefined {
	const isUpperBound = version.startsWith("≤");
	const stripped = isUpperBound ? version.slice(1) : version;
	const parts = stripped.split(".").map(Number);
	if (parts.some(Number.isNaN)) return undefined;
	return { upperBound: isUpperBound, parts };
}

/** Compares semver-ish browser versions. Handles `≤` upper-bound markers. */
function compareBrowserVersions(left: string, right: string): number {
	const parsedLeft = parseBrowserVersion(left);
	const parsedRight = parseBrowserVersion(right);
	if (!parsedLeft || !parsedRight) return 0;

	const maxLength = Math.max(parsedLeft.parts.length, parsedRight.parts.length);
	for (let index = 0; index < maxLength; index++) {
		const leftPart = parsedLeft.parts[index] ?? 0;
		const rightPart = parsedRight.parts[index] ?? 0;
		if (leftPart !== rightPart) return leftPart - rightPart;
	}

	return (parsedLeft.upperBound ? 0 : 1) - (parsedRight.upperBound ? 0 : 1);
}

function mergeBrowserVersion(
	existing: string | undefined,
	incoming: string | undefined,
): string | undefined {
	if (incoming === undefined) return existing;
	if (existing === undefined) return incoming;
	if (compareBrowserVersions(incoming, existing) > 0) return incoming;
	return existing;
}

function mergeBrowserSupport(
	existing: BrowserSupport | undefined,
	incoming: BrowserSupport,
): BrowserSupport {
	if (!existing) return { ...incoming };
	return {
		chrome: mergeBrowserVersion(existing.chrome, incoming.chrome),
		edge: mergeBrowserVersion(existing.edge, incoming.edge),
		firefox: mergeBrowserVersion(existing.firefox, incoming.firefox),
		safari: mergeBrowserVersion(existing.safari, incoming.safari),
	};
}

/**
 * Merges an attribute compat entry from one element into the global
 * attribute map. Consensus merge: deprecation/experimental only
 * become true when every observed element agrees.
 */
function mergeAttributeEntry(
	attributes: Map<string, AttributeEntry>,
	attributeName: string,
	elementName: string,
	compat: CompatEntry,
): void {
	const existing = attributes.get(attributeName);
	if (!existing) {
		attributes.set(attributeName, { ...compat, elements: [elementName] });
		return;
	}

	existing.deprecated = existing.deprecated && compat.deprecated;
	existing.experimental = existing.experimental && compat.experimental;
	if (!compat.standard_track) existing.standard_track = false;
	if (!existing.description && compat.description) existing.description = compat.description;
	if (!existing.mdn_url && compat.mdn_url) existing.mdn_url = compat.mdn_url;
	for (const url of compat.spec_url) {
		if (!existing.spec_url.includes(url)) existing.spec_url.push(url);
	}
	if (!existing.elements.includes("*") && !existing.elements.includes(elementName)) {
		existing.elements.push(elementName);
	}

	if (!existing.baseline) {
		existing.baseline = compat.baseline;
	} else if (compat.baseline) {
		const existingRank = baselineRank(existing.baseline);
		const incomingRank = baselineRank(compat.baseline);
		if (
			incomingRank < existingRank
			|| (incomingRank === existingRank
				&& (compat.baseline.since ?? 0) > (existing.baseline.since ?? 0))
		) {
			existing.baseline = compat.baseline;
		}
	}

	if (compat.browser_support) {
		existing.browser_support = mergeBrowserSupport(
			existing.browser_support,
			compat.browser_support,
		);
	}
}

/** Walks `bcd.svg.elements`, extracts `__compat` for each, returns sorted record. */
function collectElements(
	svgElements: JsonRecord,
	featureMap: JsonRecord,
): Record<string, CompatEntry> {
	const result: Record<string, CompatEntry> = {};
	const names = Object.keys(svgElements).filter((key) => key !== "__compat").sort();
	for (const name of names) {
		const element = getRecordProperty(svgElements, name);
		if (!element) continue;
		const compat = getCompat(element);
		if (!compat) continue;
		result[name] = makeCompatEntry(compat, featureMap, `svg.elements.${name}`);
	}
	return result;
}

/** Collects global + element-specific attributes from BCD, merges across elements, returns sorted. */
function collectAttributes(
	svgRoot: JsonRecord,
	featureMap: JsonRecord,
): Record<string, AttributeEntry> {
	const attributes = new Map<string, AttributeEntry>();

	const globalAttributes = getRecordProperty(svgRoot, "global_attributes");
	if (globalAttributes) {
		for (const [name, value] of Object.entries(globalAttributes)) {
			if (name === "__compat") continue;
			if (!isRecord(value)) continue;
			const compat = getCompat(value);
			if (!compat) continue;
			const canonicalName = canonicalAttributeName(name);
			const entry = makeCompatEntry(compat, featureMap, `svg.global_attributes.${name}`);
			attributes.set(canonicalName, { ...entry, elements: ["*"] });
		}
	}

	const elements = getRecordProperty(svgRoot, "elements");
	if (elements) {
		for (const [elementName, value] of Object.entries(elements)) {
			if (elementName === "__compat") continue;
			if (!isRecord(value)) continue;
			for (const [attributeName, attributeValue] of Object.entries(value)) {
				if (attributeName === "__compat") continue;
				if (!isRecord(attributeValue)) continue;
				const compat = getCompat(attributeValue);
				if (!compat) continue;
				const canonicalName = canonicalAttributeName(attributeName);
				const entry = makeCompatEntry(
					compat,
					featureMap,
					`svg.elements.${elementName}.${attributeName}`,
				);
				mergeAttributeEntry(attributes, canonicalName, elementName, entry);
			}
		}
	}

	return Object.fromEntries(
		[...attributes.entries()].sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0)),
	);
}

/** Processes raw loaded source data into a processed snapshot of elements and attributes. */
export function buildSnapshot(data: LoadedSourceData): SvgCompatSnapshot {
	const elements = getRecordProperty(data.svgRoot, "elements");
	if (!elements) {
		throw new UpstreamSourceError("BCD payload is missing the svg.elements map.");
	}

	return {
		sources: data.sources,
		elements: collectElements(elements, data.featureMap),
		attributes: collectAttributes(data.svgRoot, data.featureMap),
	};
}

/**
 * Wraps a snapshot with a generation timestamp into the final
 * output shape. `generatedAt` is required so the lib has no
 * dependency on `http.ts` / `BOOT` / `DEV` — each caller (server,
 * CLI) provides its own ISO string.
 */
export function buildOutput(
	snapshot: SvgCompatSnapshot,
	generatedAt: string,
): SvgCompatOutput {
	return {
		generated_at: generatedAt,
		sources: snapshot.sources,
		elements: snapshot.elements,
		attributes: snapshot.attributes,
	};
}
