/**
 * SVG compat data server — extracts and merges SVG element/attribute
 * compatibility data from BCD and web-features into a single JSON endpoint.
 *
 * @module
 */

import { escape } from "@std/html";
import { serveDir } from "@std/http";
import { contentType } from "@std/media-types";
import {
	InvalidSourceRequestError,
	isRecord,
	type JsonRecord,
	type LoadedSourceData,
	loadSourceDataForSelection,
	parseSourceSelection,
	type SvgCompatSources,
	UpstreamSourceError,
} from "./sources.ts";

export type { SourceInfo, SvgCompatSources } from "./sources.ts";

/** BCD uses underscore-delimited xlink names; SVG uses colon-delimited. */
const XLINK_MAP: Record<string, string> = {
	xlink_actuate: "xlink:actuate",
	xlink_arcrole: "xlink:arcrole",
	xlink_href: "xlink:href",
	xlink_role: "xlink:role",
	xlink_show: "xlink:show",
	xlink_title: "xlink:title",
	xlink_type: "xlink:type",
};

const DEV = !Deno.env.get("DENO_DEPLOYMENT_ID");
const BOOT = Date.now();
const SNAPSHOT_CACHE_TTL_MS = 12 * 60 * 60 * 1000;

interface CachedSnapshot {
	output: SvgCompatOutput;
	expiresAt: number;
}

const snapshotCache = new Map<string, CachedSnapshot>();
const snapshotInflight = new Map<string, Promise<SvgCompatSnapshot>>();
const RESPONSE_CACHE_CONTROL = "public, max-age=300, s-maxage=300, stale-while-revalidate=3600";
const SCHEMA_CACHE_CONTROL = "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400";
const CONTENT_TYPE_JSON = contentType(".json");
const CONTENT_TYPE_HTML = contentType(".html");
const CONTENT_TYPE_TEXT = contentType(".txt");

function canonicalAttributeName(name: string): string {
	return XLINK_MAP[name] ?? name;
}

/** Web-platform baseline status resolved from the `web-features` dataset. */
export interface Baseline {
	/** Baseline tier: widely available, newly available, or limited support. */
	status: "widely" | "newly" | "limited";
	/** Year the feature reached this baseline tier. */
	since?: number;
	/** ISO date when the feature first reached baseline (low tier). */
	low_date?: string;
	/** ISO date when the feature reached baseline high tier. */
	high_date?: string;
}

/** Minimum browser versions that support a feature, from BCD `support` block. */
export interface BrowserSupport {
	/** Minimum Chrome desktop version. */
	chrome?: string;
	/** Minimum Edge version. */
	edge?: string;
	/** Minimum Firefox desktop version. */
	firefox?: string;
	/** Minimum Safari desktop version. */
	safari?: string;
}

/** Processed compatibility entry for an SVG element or attribute. */
export interface CompatEntry {
	/** Human-readable feature description from BCD. */
	description?: string;
	/** MDN documentation URL. */
	mdn_url?: string;
	/** Whether the feature is deprecated. */
	deprecated: boolean;
	/** Whether the feature is experimental (single-implementer). */
	experimental: boolean;
	/** Whether the feature is on a standards track. */
	standard_track: boolean;
	/** Specification URLs from BCD. */
	spec_url: string[];
	/** Baseline status from web-features. */
	baseline?: Baseline;
	/** Minimum browser versions from BCD. */
	browser_support?: BrowserSupport;
}

/** Compat entry for an attribute, with the list of elements it applies to (`["*"]` = global). */
export interface AttributeEntry extends CompatEntry {
	/** Element names this attribute applies to. `["*"]` means global. */
	elements: string[];
}

/** Top-level JSON response shape served at `/data.json`. */
export interface SvgCompatOutput {
	/** ISO timestamp of when this output was generated. */
	generated_at: string;
	/** Upstream package versions used to build this response. */
	sources: SvgCompatSources;
	/** SVG elements keyed by tag name. */
	elements: Record<string, CompatEntry>;
	/** SVG attributes keyed by attribute name (xlink uses colon notation). */
	attributes: Record<string, AttributeEntry>;
}

/** Internal snapshot before timestamp is added. Used as the cache value. */
export interface SvgCompatSnapshot {
	/** Resolved upstream package versions. */
	sources: SvgCompatSources;
	/** Processed SVG elements. */
	elements: Record<string, CompatEntry>;
	/** Processed SVG attributes. */
	attributes: Record<string, AttributeEntry>;
}

/** JSON Schema (2020-12) describing the `/data.json` response shape. Served at `/schema.json`. */
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
				since: { type: "integer" },
				low_date: { type: "string" },
				high_date: { type: "string" },
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
};

interface NamedCompatEntry extends CompatEntry {
	name: string;
}

interface NamedAttributeEntry extends AttributeEntry {
	name: string;
}

function getString(value: unknown): string | undefined {
	return typeof value === "string" ? value : undefined;
}

function getBoolean(value: unknown): boolean | undefined {
	return typeof value === "boolean" ? value : undefined;
}

function getStringArray(value: unknown): string[] | undefined {
	if (!Array.isArray(value)) return undefined;
	const strings = value.filter((entry): entry is string => typeof entry === "string");
	return strings.length === value.length ? strings : undefined;
}

function getRecord(value: unknown): JsonRecord | undefined {
	return isRecord(value) ? value : undefined;
}

function getRecordProperty(record: JsonRecord, key: string): JsonRecord | undefined {
	return getRecord(record[key]);
}

function getCompat(node: JsonRecord): JsonRecord | undefined {
	return getRecordProperty(node, "__compat");
}

/** Resolves baseline from web-features using `web-features:` tags in BCD `__compat.tags`. */
function extractBaseline(
	compat: JsonRecord,
	featureMap: JsonRecord,
	compatKey: string,
): Baseline | undefined {
	const tags = getStringArray(compat.tags);
	if (!tags) return undefined;

	const featureTag = tags.find((tag) => tag.startsWith("web-features:"));
	if (!featureTag) return undefined;

	const featureId = featureTag.slice("web-features:".length);
	const feature = getRecordProperty(featureMap, featureId);
	if (!feature || getString(feature.kind) !== "feature") return undefined;

	const status = getRecordProperty(feature, "status");
	if (!status) return undefined;

	const byCompatKey = getRecordProperty(status, "by_compat_key");
	const overrideStatus = byCompatKey ? getRecordProperty(byCompatKey, compatKey) : undefined;
	if (overrideStatus) return parseBaseline(overrideStatus);

	return parseBaseline(status);
}

function parseBaseline(status: JsonRecord): Baseline | undefined {
	const baseline = status.baseline;
	if (baseline === false) return { status: "limited" };
	if (baseline === "high") {
		const since = extractYear(getString(status.baseline_high_date));
		if (since === undefined) return undefined;
		return {
			status: "widely",
			since,
			low_date: getString(status.baseline_low_date),
			high_date: getString(status.baseline_high_date),
		};
	}
	if (baseline === "low") {
		const since = extractYear(getString(status.baseline_low_date));
		if (since === undefined) return undefined;
		return {
			status: "newly",
			since,
			low_date: getString(status.baseline_low_date),
		};
	}
	return undefined;
}

function extractYear(date: string | undefined): number | undefined {
	if (!date) return undefined;
	const [yearText] = date.split("-");
	if (!yearText) return undefined;
	const year = parseInt(yearText, 10);
	return Number.isNaN(year) ? undefined : year;
}

function browserVersion(value: unknown): string | undefined {
	if (isRecord(value)) return getString(value.version_added);
	if (!Array.isArray(value)) return undefined;
	for (const entry of value) {
		if (!isRecord(entry)) continue;
		const version = getString(entry.version_added);
		if (version !== undefined) return version;
	}
	return undefined;
}

function extractBrowserSupport(compat: JsonRecord): BrowserSupport | undefined {
	const support = getRecordProperty(compat, "support");
	if (!support) return undefined;

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

function extractSpecUrls(compat: JsonRecord): string[] {
	const url = compat.spec_url;
	if (typeof url === "string") return [url];
	if (!Array.isArray(url)) return [];
	return url.filter((entry): entry is string => typeof entry === "string");
}

/** Builds a {@linkcode CompatEntry} from a BCD `__compat` node and web-features lookup. */
function makeCompatEntry(
	compat: JsonRecord,
	featureMap: JsonRecord,
	compatKey: string,
): CompatEntry {
	const status = getRecordProperty(compat, "status");
	return {
		description: getString(compat.description),
		mdn_url: getString(compat.mdn_url),
		deprecated: getBoolean(status?.deprecated) ?? false,
		experimental: getBoolean(status?.experimental) ?? false,
		standard_track: getBoolean(status?.standard_track) ?? true,
		spec_url: extractSpecUrls(compat),
		baseline: extractBaseline(compat, featureMap, compatKey),
		browser_support: extractBrowserSupport(compat),
	};
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
 * Merges an attribute compat entry from one element into the global attribute map.
 * Pessimistic: deprecated/experimental use OR, baseline picks worst, browser versions take latest.
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

	if (compat.deprecated) existing.deprecated = true;
	if (compat.experimental) existing.experimental = true;
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
function buildSnapshot(data: LoadedSourceData): SvgCompatSnapshot {
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
 * Wraps a snapshot with a generation timestamp into the final output shape.
 *
 * @param snapshot Processed element/attribute data
 * @param generatedAt ISO timestamp, defaults to now
 * @returns The complete JSON response object
 */
export function buildOutput(
	snapshot: SvgCompatSnapshot,
	generatedAt: string = new Date().toISOString(),
): SvgCompatOutput {
	return {
		generated_at: generatedAt,
		sources: snapshot.sources,
		elements: snapshot.elements,
		attributes: snapshot.attributes,
	};
}

/** Returns cached or freshly-built output for the given request URL's source selection. */
async function getOutput(url: URL): Promise<SvgCompatOutput> {
	const selection = parseSourceSelection(url);
	const key = `${selection.bcd}|${selection.wf}`;
	const cached = snapshotCache.get(key);
	if (!DEV && cached && cached.expiresAt > Date.now()) return cached.output;

	const inflight = snapshotInflight.get(key);
	if (inflight) {
		const snapshot = await inflight;
		return buildOutput(snapshot);
	}

	const promise = (async () => {
		const sourceData = await loadSourceDataForSelection(selection);
		return buildSnapshot(sourceData);
	})();

	snapshotInflight.set(key, promise);
	try {
		const snapshot = await promise;
		const output = buildOutput(snapshot);
		snapshotCache.set(key, {
			output,
			expiresAt: Date.now() + SNAPSHOT_CACHE_TTL_MS,
		});
		return output;
	} finally {
		snapshotInflight.delete(key);
	}
}

function entityTag(value: string): string {
	return `W/"${value}"`;
}

function outputEtag(output: SvgCompatOutput): string {
	return entityTag(
		[
			output.sources.bcd.resolved,
			output.sources.web_features.resolved,
			output.generated_at,
		].join("|"),
	);
}

function schemaEtag(): string {
	return entityTag("svg-compat-schema-v1");
}

function requestMatchesCache(request: Request, etag: string, lastModified: string): boolean {
	const ifNoneMatch = request.headers.get("if-none-match");
	if (ifNoneMatch) {
		return ifNoneMatch.split(",").map((value) => value.trim()).includes(etag);
	}

	const ifModifiedSince = request.headers.get("if-modified-since");
	if (!ifModifiedSince) return false;
	const requestTime = Date.parse(ifModifiedSince);
	const modifiedTime = Date.parse(lastModified);
	if (Number.isNaN(requestTime) || Number.isNaN(modifiedTime)) return false;
	return requestTime >= modifiedTime;
}

function cachedResponse(
	request: Request,
	body: string,
	contentType: string,
	cacheControl: string,
	etag: string,
	lastModified: string,
	status = 200,
): Response {
	if (DEV) {
		return new Response(body, {
			status,
			headers: { "content-type": contentType, "cache-control": "no-store" },
		});
	}
	const headers = new Headers({
		"content-type": contentType,
		"cache-control": cacheControl,
		etag,
		"last-modified": lastModified,
		vary: "accept",
	});
	if (requestMatchesCache(request, etag, lastModified)) {
		return new Response(null, { status: 304, headers });
	}
	return new Response(body, { status, headers });
}

const RELOAD_SCRIPT = DEV
	? `<script>((b)=>{setInterval(()=>fetch("/__reload").then(r=>r.text()).then(t=>{if(t!==b){b=t;location.reload()}}),500)})(${
		JSON.stringify(String(BOOT))
	})</script>`
	: "";

function formatBaseline(baseline: Baseline | undefined): string {
	if (!baseline) return "-";
	if (baseline.status === "limited") return "limited";
	return `${baseline.status} (${baseline.since ?? "?"})`;
}

const BASELINE_ICON_HIGH =
	`<svg xmlns="http://www.w3.org/2000/svg" width="18" height="10" viewBox="0 0 540 300" fill="none"><path fill="#c4eed0" d="M420 30L390 60L480 150L390 240L330 180L300 210L390 300L540 150L420 30Z"/><path fill="#c4eed0" d="M150 0L30 120L60 150L150 60L210 120L240 90L150 0Z"/><path d="M390 0L420 30L150 300L0 150L30 120L150 240L390 0Z" fill="#1EA446"/></svg>`;
const BASELINE_ICON_LOW =
	`<svg xmlns="http://www.w3.org/2000/svg" width="18" height="10" viewBox="0 0 540 300" fill="none"><path fill="#a8c7fa" d="M150 0L180 30L150 60L120 30L150 0Z"/><path fill="#a8c7fa" d="M210 60L240 90L210 120L180 90L210 60Z"/><path fill="#a8c7fa" d="M450 60L480 90L450 120L420 90L450 60Z"/><path fill="#a8c7fa" d="M510 120L540 150L510 180L480 150L510 120Z"/><path fill="#a8c7fa" d="M450 180L480 210L450 240L420 210L450 180Z"/><path fill="#a8c7fa" d="M390 240L420 270L390 300L360 270L390 240Z"/><path fill="#a8c7fa" d="M330 180L360 210L330 240L300 210L330 180Z"/><path fill="#a8c7fa" d="M90 60L120 90L90 120L60 90L90 60Z"/><path fill="#4185ff" d="M390 0L420 30L150 300L0 150L30 120L150 240L390 0Z"/></svg>`;
const BASELINE_ICON_LIMITED =
	`<svg xmlns="http://www.w3.org/2000/svg" width="18" height="10" viewBox="0 0 540 300" fill="none"><path d="M150 0L240 90L210 120L120 30L150 0Z" fill="#F09409"/><path fill="#565656" d="M420 30L540 150L420 270L390 240L480 150L390 60L420 30Z"/><path d="M330 180L300 210L390 300L420 270L330 180Z" fill="#F09409"/><path fill="#565656" d="M120 30L150 60L60 150L150 240L120 270L0 150L120 30Z"/><path d="M390 0L420 30L150 300L120 270L390 0Z" fill="#F09409"/></svg>`;

function renderBaselineBadge(baseline: Baseline | undefined): string {
	if (!baseline) return `<span class="muted">-</span>`;
	const icon = baseline.status === "widely"
		? BASELINE_ICON_HIGH
		: baseline.status === "newly"
		? BASELINE_ICON_LOW
		: BASELINE_ICON_LIMITED;
	const variant = baseline.status === "widely"
		? "widely"
		: baseline.status === "newly"
		? "newly"
		: "limited";
	const label = baseline.status === "limited"
		? "limited"
		: `${baseline.status} ${baseline.since ?? ""}`.trim();
	return `<span class="badge badge-${variant}">${icon} ${escape(label)}</span>`;
}

function formatBrowserSupport(browserSupport: BrowserSupport | undefined): string {
	if (!browserSupport) return "-";
	const parts = [
		browserSupport.chrome ? `Chrome ${browserSupport.chrome}` : undefined,
		browserSupport.edge ? `Edge ${browserSupport.edge}` : undefined,
		browserSupport.firefox ? `Firefox ${browserSupport.firefox}` : undefined,
		browserSupport.safari ? `Safari ${browserSupport.safari}` : undefined,
	].filter((value): value is string => value !== undefined);
	return parts.length > 0 ? parts.join(" | ") : "-";
}

function allElements(output: SvgCompatOutput): NamedCompatEntry[] {
	return Object.entries(output.elements)
		.map(([name, entry]) => ({ name, ...entry }));
}

function allAttributes(output: SvgCompatOutput): NamedAttributeEntry[] {
	return Object.entries(output.attributes)
		.map(([name, entry]) => ({ name, ...entry }));
}

function deprecatedEntries(output: SvgCompatOutput): NamedCompatEntry[] {
	return Object.entries(output.elements)
		.filter(([, entry]) => entry.deprecated)
		.map(([name, entry]) => ({ name, ...entry }));
}

function limitedAttributes(output: SvgCompatOutput): NamedAttributeEntry[] {
	return Object.entries(output.attributes)
		.filter(([, entry]) => entry.baseline?.status === "limited")
		.map(([name, entry]) => ({ name, ...entry }));
}

function renderCompatRows(entries: NamedCompatEntry[]): string {
	return entries
		.map((entry) => {
			const mdnCell = entry.mdn_url
				? `<a href="${escape(entry.mdn_url)}">MDN</a>`
				: "-";
			return `<tr><th scope="row"><code>${escape(entry.name)}</code></th><td>${
				renderBaselineBadge(entry.baseline)
			}</td><td>${escape(formatBrowserSupport(entry.browser_support))}</td><td>${mdnCell}</td></tr>`;
		})
		.join("");
}

function renderAttributeRows(entries: NamedAttributeEntry[]): string {
	return entries
		.map((entry) => {
			const mdnCell = entry.mdn_url
				? `<a href="${escape(entry.mdn_url)}">MDN</a>`
				: "-";
			const elements = entry.elements.length === 1 && entry.elements[0] === "*"
				? "global"
				: entry.elements.join(", ");
			return `<tr><th scope="row"><code>${escape(entry.name)}</code></th><td>${escape(elements)}</td><td>${
				renderBaselineBadge(entry.baseline)
			}</td><td>${mdnCell}</td></tr>`;
		})
		.join("");
}

function renderSourceRows(output: SvgCompatOutput): string {
	return Object.values(output.sources)
		.map((source) => {
			return `<tr><th scope="row"><code>${escape(source.package)}</code></th><td>${escape(source.requested)}</td><td>${
				escape(source.resolved)
			}</td><td>${escape(source.mode)}</td><td><a href="${escape(source.source_url)}">source</a></td></tr>`;
		})
		.join("");
}

/**
 * Renders the HTML dashboard page with stats, element/attribute tables, and source info.
 *
 * @param output The processed compat data
 * @param requestUrl Used to build links to JSON/schema endpoints
 * @returns Complete HTML string
 */
export function renderHtml(output: SvgCompatOutput, requestUrl: URL): string {
	const elementCount = Object.keys(output.elements).length;
	const attributeCount = Object.keys(output.attributes).length;
	const deprecatedCount = Object.values(output.elements).filter((entry) => entry.deprecated).length;
	const limitedCount = Object.values(output.attributes).filter((entry) => entry.baseline?.status === "limited").length;
	const jsonUrl = `${requestUrl.origin}/data.json`;
	const schemaUrl = `${requestUrl.origin}/schema.json`;
	const latestHtmlUrl = `${requestUrl.origin}/?source=latest`;
	const latestJsonUrl = `${requestUrl.origin}/data.json?source=latest`;

	return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>SVG Compat</title>
  <link rel="icon" href="/static/favicon.svg" type="image/svg+xml">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@500;700&family=JetBrains+Mono:wght@400;500&display=swap">
  <link rel="stylesheet" href="/static/style.css">
</head>
<body>
  <main>
    <div class="hero">
      <p class="muted">SVG compatibility catalog</p>
      <h1>Browser face. Dynamic source knobs.</h1>
      <div class="muted">Generated ${escape(output.generated_at)}.<br>
        Ask for <code>/data.json</code>, <code>?format=json</code>, or <code>Accept: application/json</code>.<br>
        Override upstream packages with <code>?source=latest</code> or explicit
        <code>?bcd=</code>
        <span class="ver-wrap">
          <input type="text" class="ver" data-pkg="@mdn/browser-compat-data" data-param="bcd" placeholder="version" autocomplete="off" aria-label="BCD version">
          <ul class="ver-list" hidden></ul>
        </span>
        <code>&amp;wf=</code>
        <span class="ver-wrap">
          <input type="text" class="ver" data-pkg="web-features" data-param="wf" placeholder="version" autocomplete="off" aria-label="Web Features version">
          <ul class="ver-list" hidden></ul>
        </span>.
      </div>
      <p>
        <a class="pill" href="${escape(jsonUrl)}">Open JSON endpoint</a>
        <a class="pill" href="${escape(schemaUrl)}">Open schema</a>
        <a class="pill" href="${escape(latestHtmlUrl)}">Try latest in browser</a>
        <a class="pill" href="${escape(latestJsonUrl)}">Try latest JSON</a>
      </p>
      <div class="stats">
        <div class="stat"><strong>${elementCount}</strong><span class="muted">elements</span></div>
        <div class="stat"><strong>${attributeCount}</strong><span class="muted">attributes</span></div>
        <div class="stat"><strong>${deprecatedCount}</strong><span class="muted">deprecated elements</span></div>
        <div class="stat"><strong>${limitedCount}</strong><span class="muted">limited attributes</span></div>
      </div>
    </div>
    <div class="grid" style="margin-top: 16px;">
      <section>
        <h2>Upstream sources</h2>
        <p class="muted">Effective package versions for this exact response.</p>
        <table>
          <thead>
            <tr><th>Package</th><th>Requested</th><th>Resolved</th><th>Mode</th><th>Link</th></tr>
          </thead>
          <tbody>${renderSourceRows(output)}</tbody>
        </table>
      </section>
      <section>
        <h2>Elements</h2>
        <p class="muted">All elements with baseline and browser floor.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Baseline</th><th>Support</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderCompatRows(allElements(output))}</tbody>
        </table>
      </section>
      <section>
        <h2>Attributes</h2>
        <p class="muted">All attributes with scope and baseline.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Elements</th><th>Baseline</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderAttributeRows(allAttributes(output))}</tbody>
        </table>
      </section>
      <section>
        <h2>Deprecated elements</h2>
        <p class="muted">Quick smoke panel for legacy SVG pieces.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Baseline</th><th>Support</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderCompatRows(deprecatedEntries(output))}</tbody>
        </table>
      </section>
      <section>
        <h2>Limited attributes</h2>
        <p class="muted">Useful for cross-browser pain radar.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Elements</th><th>Baseline</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderAttributeRows(limitedAttributes(output))}</tbody>
        </table>
      </section>
    </div>
  </main>
  <script type="module" src="/static/version-picker.js"></script>
  ${RELOAD_SCRIPT}
</body>
</html>`;
}

function renderErrorHtml(status: number, message: string): string {
	return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>SVG Compat Error</title>
  <link rel="stylesheet" href="/static/style.css">
</head>
<body>
  <main class="error-page">
    <article>
      <p><strong>${status}</strong></p>
      <h1>Request failed</h1>
      <p>${escape(message)}</p>
      <p>Ask for <code>application/json</code>, <code>/data.json</code>, or <code>text/html</code>.</p>
    </article>
  </main>
  ${RELOAD_SCRIPT}
</body>
</html>`;
}

function wantsJson(request: Request, url: URL): boolean {
	if (url.pathname === "/data.json") return true;
	if (url.pathname === "/schema.json") return true;
	if (url.searchParams.get("format") === "json") return true;
	const accept = request.headers.get("accept");
	if (!accept) return false;
	return accept.includes("application/json");
}

function wantsHtml(request: Request): boolean {
	const accept = request.headers.get("accept");
	if (!accept) return false;
	return accept.includes("text/html");
}

function classifyError(error: unknown): { status: number; message: string } {
	if (error instanceof InvalidSourceRequestError) {
		return { status: 400, message: error.message };
	}
	if (error instanceof UpstreamSourceError) {
		return { status: 502, message: error.message };
	}
	return { status: 500, message: "Internal server error." };
}

const schemaBody = JSON.stringify(SVG_COMPAT_SCHEMA, null, "  ");
const schemaLastModified = new Date("2026-04-11T00:00:00.000Z").toUTCString();
const schemaTag = schemaEtag();

/** Deno serve-compatible handler. */
export interface Server {
	/** Handles an incoming HTTP request. */
	fetch(request: Request): Promise<Response>;
}

/** The default export for `deno serve`. */
const server: Server = {
	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);
		if (request.method !== "GET" && request.method !== "HEAD") {
			return new Response("Method not allowed", {
				status: 405,
				headers: { allow: "GET, HEAD" },
			});
		}

		if (DEV && url.pathname === "/__reload") {
			return new Response(String(BOOT), {
				headers: { "content-type": CONTENT_TYPE_TEXT, "cache-control": "no-store" },
			});
		}

		if (url.pathname.startsWith("/static/")) {
			return serveDir(request, {
				fsRoot: new URL("../static", import.meta.url).pathname,
				urlRoot: "static",
			});
		}

		if (wantsJson(request, url) && url.pathname === "/schema.json") {
			return cachedResponse(
				request,
				schemaBody,
				"application/schema+json; charset=utf-8",
				SCHEMA_CACHE_CONTROL,
				schemaTag,
				schemaLastModified,
			);
		}

		if (!wantsJson(request, url) && !wantsHtml(request)) {
			return new Response(
				"Not acceptable. Ask for application/json, /data.json, ?format=json, or text/html.",
				{
					status: 406,
					headers: { "content-type": CONTENT_TYPE_TEXT },
				},
			);
		}

		try {
			const output = await getOutput(url);
			const etag = outputEtag(output);
			const lastModified = new Date(output.generated_at).toUTCString();

			if (wantsJson(request, url)) {
				return cachedResponse(
					request,
					JSON.stringify(output, null, "  "),
					CONTENT_TYPE_JSON,
					RESPONSE_CACHE_CONTROL,
					etag,
					lastModified,
				);
			}

			return cachedResponse(
				request,
				renderHtml(output, url),
				CONTENT_TYPE_HTML,
				RESPONSE_CACHE_CONTROL,
				etag,
				lastModified,
			);
		} catch (error) {
			const { status, message } = classifyError(error);
			if (wantsJson(request, url)) {
				return new Response(JSON.stringify({ error: { status, message } }, null, "  "), {
					status,
					headers: { "content-type": CONTENT_TYPE_JSON },
				});
			}

			if (wantsHtml(request)) {
				return new Response(renderErrorHtml(status, message), {
					status,
					headers: { "content-type": CONTENT_TYPE_HTML },
				});
			}

			return new Response(message, {
				status,
				headers: { "content-type": CONTENT_TYPE_TEXT },
			});
		}
	},
} satisfies Deno.ServeDefaultExport;

export default server;
