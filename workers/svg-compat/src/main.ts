/**
 * SVG compat data server — extracts and merges SVG element/attribute
 * compatibility data from BCD and web-features into a single JSON endpoint.
 *
 * @module
 */

import {
	applyCommonSecurityHeaders,
	applyHtmlSecurityHeaders,
	BOOT,
	CACHE_POLICY,
	cachedResponse,
	DEV,
	entityTag,
	hashString,
	jsonErrorResponse,
	loadStaticAsset,
	MIME,
	negotiateFormat,
	serveStaticRoute,
	staticAssetResponse,
	textResponse,
} from "./http.ts";
import { renderErrorHtml, renderHtml } from "./render.tsx";
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

const snapshotInflight = new Map<string, Promise<SvgCompatSnapshot>>();
const JSON_INDENT = 2;
const WEB_FEATURE_KIND_FEATURE = "feature";
const PROD_GENERATED_AT = new Date(BOOT).toISOString();

const FAVICON_ICO_ASSET = loadStaticAsset("favicon.ico", MIME.ico);
const FAVICON_SVG_ASSET = loadStaticAsset("favicon.svg", MIME.svg);

/**
 * BCD uses underscore-delimited namespace names (`xlink_href`, `xml_lang`);
 * SVG uses colon-delimited (`xlink:href`, `xml:lang`). Canonicalise here.
 * Regex subsumes the old hand-written XLINK_MAP and auto-covers any future
 * `xml_*` / `xlink_*` additions upstream.
 */
const NAMESPACE_UNDERSCORE = /^(xlink|xml)_(\w+)$/;

function canonicalAttributeName(name: string): string {
	const match = name.match(NAMESPACE_UNDERSCORE);
	return match ? `${match[1]}:${match[2]}` : name;
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

const loggedWarnings = new Set<string>();

function stringifyUnknown(value: unknown): string {
	if (typeof value === "string") return JSON.stringify(value);
	if (typeof value === "number" || typeof value === "boolean") return String(value);
	if (value === null) return "null";
	if (value === undefined) return "undefined";
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

function warnOnce(key: string, message: string): void {
	if (loggedWarnings.has(key)) return;
	loggedWarnings.add(key);
	console.warn(message);
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
	if (!feature) return undefined;
	const featureKind = getString(feature.kind);
	if (featureKind !== WEB_FEATURE_KIND_FEATURE) {
		warnOnce(
			`wf-kind:${featureKind ?? "<missing>"}`,
			`svg-compat: unsupported web-features kind ${stringifyUnknown(featureKind)} for "${featureId}".`,
		);
		return undefined;
	}

	const status = getRecordProperty(feature, "status");
	if (!status) return undefined;

	const byCompatKey = getRecordProperty(status, "by_compat_key");
	const overrideStatus = byCompatKey ? getRecordProperty(byCompatKey, compatKey) : undefined;
	if (overrideStatus) return parseBaseline(overrideStatus, compatKey);

	return parseBaseline(status, compatKey);
}

function parseBaseline(status: JsonRecord, compatKey: string): Baseline | undefined {
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
	if (baseline !== undefined) {
		warnOnce(
			`wf-baseline:${stringifyUnknown(baseline)}`,
			`svg-compat: unsupported baseline value ${stringifyUnknown(baseline)} for "${compatKey}".`,
		);
	}
	return undefined;
}

function extractYear(date: string | undefined): number | undefined {
	if (!date) return undefined;
	const parsed = Date.parse(date);
	if (Number.isNaN(parsed)) return undefined;
	return new Date(parsed).getUTCFullYear();
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
 * Consensus merge: deprecation/experimental only become true when every observed element agrees.
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
	generatedAt: string = DEV ? new Date().toISOString() : PROD_GENERATED_AT,
): SvgCompatOutput {
	return {
		generated_at: generatedAt,
		sources: snapshot.sources,
		elements: snapshot.elements,
		attributes: snapshot.attributes,
	};
}

/** Returns freshly-built output for the given request URL's source selection. */
async function getOutput(url: URL): Promise<SvgCompatOutput> {
	const selection = parseSourceSelection(url);
	const key = `${selection.bcd}|${selection.wf}`;

	if (!DEV) {
		const inflight = snapshotInflight.get(key);
		if (inflight) {
			const snapshot = await inflight;
			return buildOutput(snapshot);
		}
	}

	const promise = loadSourceDataForSelection(selection).then(buildSnapshot);

	if (!DEV) snapshotInflight.set(key, promise);
	try {
		const snapshot = await promise;
		return buildOutput(snapshot);
	} finally {
		if (!DEV) snapshotInflight.delete(key);
	}
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
	return entityTag(`svg-compat-schema-${hashString(schemaBody)}`);
}

function htmlErrorResponse(status: number, message: string): Response {
	const headers = new Headers({
		"content-type": MIME.html,
		"cache-control": "no-store",
	});
	applyHtmlSecurityHeaders(headers);
	return new Response(renderErrorHtml(status, message, DEV, BOOT), { status, headers });
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

const schemaBody = JSON.stringify(SVG_COMPAT_SCHEMA, null, JSON_INDENT);
const schemaLastModified = new Date(BOOT).toUTCString();
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
			const headers = new Headers({
				allow: "GET, HEAD",
				"content-type": MIME.text,
			});
			applyCommonSecurityHeaders(headers);
			return new Response("Method not allowed", {
				status: 405,
				headers,
			});
		}

		if (DEV && url.pathname === "/__reload") {
			return textResponse(String(BOOT));
		}

		if (url.pathname === "/favicon.ico") {
			return staticAssetResponse(request, FAVICON_ICO_ASSET);
		}

		if (url.pathname === "/favicon.svg") {
			return staticAssetResponse(request, FAVICON_SVG_ASSET);
		}

		if (
			url.pathname === "/style.css"
			|| url.pathname === "/version-picker.mjs"
			|| url.pathname === "/table-filter.mjs"
			|| url.pathname.startsWith("/badges/")
			|| url.pathname.startsWith("/browsers/")
		) {
			return await serveStaticRoute(request, "");
		}

		if (url.pathname.startsWith("/static/")) {
			const redirectUrl = new URL(request.url);
			redirectUrl.pathname = url.pathname.slice("/static".length);
			return Response.redirect(redirectUrl, 308);
		}

		const format = negotiateFormat(request, url);

		if (url.pathname === "/schema.json") {
			if (format === "html") {
				return textResponse(
					"Not acceptable. /schema.json is JSON only. Ask for application/json or */*.",
					406,
				);
			}
			return cachedResponse(
				request,
				schemaBody,
				MIME.schema,
				CACHE_POLICY.schema,
				schemaTag,
				schemaLastModified,
			);
		}

		if (format === "not-acceptable") {
			return textResponse(
				"Not acceptable. Ask for application/json, /data.json, ?format=json, or text/html.",
				406,
			);
		}

		try {
			const output = await getOutput(url);
			const etag = outputEtag(output);
			const lastModified = new Date(output.generated_at).toUTCString();

			if (format === "json") {
				return cachedResponse(
					request,
					JSON.stringify(output, null, JSON_INDENT),
					MIME.json,
					CACHE_POLICY.response,
					etag,
					lastModified,
				);
			}

			return cachedResponse(
				request,
				renderHtml(output, url, DEV, BOOT),
				MIME.html,
				CACHE_POLICY.response,
				etag,
				lastModified,
			);
		} catch (error) {
			const { status, message } = classifyError(error);
			if (format === "json") {
				return jsonErrorResponse(status, message);
			}

			if (format === "html") {
				return htmlErrorResponse(status, message);
			}

			return textResponse(message, status);
		}
	},
} satisfies Deno.ServeDefaultExport;

export default server;
