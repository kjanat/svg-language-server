import bcd from "@mdn/browser-compat-data" with { type: "json" };
import type { CompatStatement, Identifier, SimpleSupportStatement, SupportStatement } from "@mdn/browser-compat-data";
import { features } from "web-features";

// --- xlink canonicalization (mirrors crates/svg-data/src/xlink.rs) ---

const XLINK_MAP: Record<string, string> = {
	xlink_actuate: "xlink:actuate",
	xlink_arcrole: "xlink:arcrole",
	xlink_href: "xlink:href",
	xlink_role: "xlink:role",
	xlink_show: "xlink:show",
	xlink_title: "xlink:title",
	xlink_type: "xlink:type",
};

function canonicalAttributeName(name: string): string {
	return XLINK_MAP[name] ?? name;
}

// --- output types ---

export interface Baseline {
	status: "widely" | "newly" | "limited";
	since?: number;
}

export interface BrowserSupport {
	chrome?: string;
	edge?: string;
	firefox?: string;
	safari?: string;
}

export interface CompatEntry {
	deprecated: boolean;
	experimental: boolean;
	spec_url?: string;
	baseline?: Baseline;
	browser_support?: BrowserSupport;
}

export interface AttributeEntry extends CompatEntry {
	elements: string[];
}

export interface SvgCompatOutput {
	generated_at: string;
	elements: Record<string, CompatEntry>;
	attributes: Record<string, AttributeEntry>;
}

// --- baseline resolution (mirrors bcd.rs extract_baseline) ---

function extractBaseline(
	compat: CompatStatement,
	compatKey: string,
): Baseline | undefined {
	const tags = compat.tags;
	if (!tags) return undefined;

	const featureTag = tags.find((t) => t.startsWith("web-features:"));
	if (!featureTag) return undefined;

	const featureId = featureTag.slice("web-features:".length);
	const feature = features[featureId];
	if (!feature || feature.kind !== "feature") return undefined;

	const status = feature.status;
	const overrideStatus = status.by_compat_key?.[compatKey];
	if (overrideStatus) return parseBaseline(overrideStatus);

	return parseBaseline(status);
}

interface BaselineStatus {
	baseline: boolean | "high" | "low";
	baseline_high_date?: string;
	baseline_low_date?: string;
}

function parseBaseline(status: BaselineStatus): Baseline | undefined {
	if (status.baseline === false) return { status: "limited" };
	if (status.baseline === "high") {
		const since = extractYear(status.baseline_high_date);
		return since !== undefined ? { status: "widely", since } : undefined;
	}
	if (status.baseline === "low") {
		const since = extractYear(status.baseline_low_date);
		return since !== undefined ? { status: "newly", since } : undefined;
	}
	return undefined;
}

function extractYear(date?: string): number | undefined {
	if (!date) return undefined;
	const year = parseInt(date.split("-")[0]!, 10);
	return Number.isNaN(year) ? undefined : year;
}

// --- browser support (mirrors bcd.rs extract_browser_support) ---

function extractBrowserSupport(
	compat: CompatStatement,
): BrowserSupport | undefined {
	const support = compat.support;

	function browserVersion(
		stmt: SupportStatement | undefined,
	): string | undefined {
		if (!stmt) return undefined;
		const entries: SimpleSupportStatement[] = Array.isArray(stmt)
			? stmt
			: [stmt];
		for (const entry of entries) {
			const v = entry.version_added;
			if (typeof v === "string") return v;
		}
		return undefined;
	}

	const chrome = browserVersion(support.chrome);
	const edge = browserVersion(support.edge);
	const firefox = browserVersion(support.firefox);
	const safari = browserVersion(support.safari);

	if (
		chrome === undefined
		&& edge === undefined
		&& firefox === undefined
		&& safari === undefined
	) {
		return undefined;
	}

	const result: BrowserSupport = {};
	if (chrome !== undefined) result.chrome = chrome;
	if (edge !== undefined) result.edge = edge;
	if (firefox !== undefined) result.firefox = firefox;
	if (safari !== undefined) result.safari = safari;
	return result;
}

// --- spec url ---

function extractSpecUrl(compat: CompatStatement): string | undefined {
	const url = compat.spec_url;
	if (typeof url === "string") return url;
	if (Array.isArray(url)) return url[0];
	return undefined;
}

// --- compat entry construction ---

function makeCompatEntry(
	compat: CompatStatement,
	compatKey: string,
): CompatEntry {
	return {
		deprecated: compat.status?.deprecated ?? false,
		/** TODO: this is deprecated, but still used by build.rs. We should probs do summit about it.
		 * @deprecated — (This property is deprecated. Prefer using more well-defined
		 * stability calculations, such as Baseline, instead.)
		 * A boolean value. Usually, this value is true for single-implementer
		 * features and false for multiple-implementer features or
		 * single-implementer features that are not expected to change.
		 */
		experimental: compat.status?.experimental ?? false,
		spec_url: extractSpecUrl(compat),
		baseline: extractBaseline(compat, compatKey),
		browser_support: extractBrowserSupport(compat),
	};
}

// --- baseline ranking for merge (mirrors bcd.rs BaselineValue::rank) ---

function baselineRank(b: Baseline): number {
	if (b.status === "limited") return 0;
	if (b.status === "newly") return 1;
	return 2; // widely
}

// --- browser version comparison (mirrors bcd.rs compare_browser_versions) ---

function parseBrowserVersion(
	version: string,
): { upperBound: boolean; parts: number[] } | undefined {
	const isUpperBound = version.startsWith("≤");
	const stripped = isUpperBound ? version.slice(1) : version;
	const parts = stripped.split(".").map(Number);
	if (parts.some(Number.isNaN)) return undefined;
	return { upperBound: isUpperBound, parts };
}

function compareBrowserVersions(left: string, right: string): number {
	const l = parseBrowserVersion(left);
	const r = parseBrowserVersion(right);
	if (!l || !r) return 0;

	const maxLen = Math.max(l.parts.length, r.parts.length);
	for (let i = 0; i < maxLen; i++) {
		const lp = l.parts[i] ?? 0;
		const rp = r.parts[i] ?? 0;
		if (lp !== rp) return lp - rp;
	}

	return (l.upperBound ? 0 : 1) - (r.upperBound ? 0 : 1);
}

// --- merge logic (mirrors bcd.rs merge_attribute_entry) ---

function mergeBrowserVersion(
	existing: string | undefined,
	incoming: string | undefined,
): string | undefined {
	if (incoming === undefined) return undefined;
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

	if (compat.deprecated) existing.deprecated = true;
	if (compat.experimental) existing.experimental = true;
	if (!existing.spec_url && compat.spec_url) {
		existing.spec_url = compat.spec_url;
	}

	if (
		!existing.elements.includes("*")
		&& !existing.elements.includes(elementName)
	) {
		existing.elements.push(elementName);
	}

	if (!existing.baseline) {
		existing.baseline = compat.baseline;
	} else if (existing.baseline && compat.baseline) {
		const existingRank = baselineRank(existing.baseline);
		const newRank = baselineRank(compat.baseline);
		if (
			newRank < existingRank
			|| (newRank === existingRank
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

// --- collection ---

function collectElements(
	svgElements: Identifier,
): Record<string, CompatEntry> {
	const result: Record<string, CompatEntry> = {};
	const names = Object.keys(svgElements).filter((k) => k !== "__compat").sort();
	for (const name of names) {
		const compat = (svgElements[name] as Identifier).__compat;
		if (!compat) continue;
		result[name] = makeCompatEntry(compat, `svg.elements.${name}`);
	}
	return result;
}

function collectAttributes(
	svgIdentifier: Identifier,
): Record<string, AttributeEntry> {
	const attributes = new Map<string, AttributeEntry>();

	const globalAttrs = svgIdentifier.global_attributes as
		| Identifier
		| undefined;
	if (globalAttrs) {
		for (const [name, data] of Object.entries(globalAttrs)) {
			if (name === "__compat") continue;
			const compat = (data as Identifier).__compat;
			if (!compat) continue;
			const canonical = canonicalAttributeName(name);
			const entry = makeCompatEntry(compat, `svg.global_attributes.${name}`);
			attributes.set(canonical, { ...entry, elements: ["*"] });
		}
	}

	const elements = svgIdentifier.elements as Identifier | undefined;
	if (elements) {
		for (const [elementName, elementData] of Object.entries(elements)) {
			if (elementName === "__compat") continue;
			const elementObj = elementData as Identifier;
			for (const [attrName, attrData] of Object.entries(elementObj)) {
				if (attrName === "__compat") continue;
				const compat = (attrData as Identifier).__compat;
				if (!compat) continue;
				const canonical = canonicalAttributeName(attrName);
				const entry = makeCompatEntry(
					compat,
					`svg.elements.${elementName}.${attrName}`,
				);
				mergeAttributeEntry(attributes, canonical, elementName, entry);
			}
		}
	}

	return Object.fromEntries(
		[...attributes.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
	);
}

// --- build (runs once at import time) ---

export function buildOutput(): SvgCompatOutput {
	const svg = bcd.svg as Identifier;
	return {
		generated_at: new Date().toISOString(),
		elements: collectElements(svg.elements as Identifier),
		attributes: collectAttributes(svg),
	};
}

const output = buildOutput();
const body = JSON.stringify(output, null, "\t");

export default {
	fetch(_req: Request): Response {
		return new Response(body, {
			headers: { "content-type": "application/json" },
		});
	},
} satisfies Deno.ServeDefaultExport;
