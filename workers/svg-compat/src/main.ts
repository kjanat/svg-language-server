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

const XLINK_MAP: Record<string, string> = {
	xlink_actuate: "xlink:actuate",
	xlink_arcrole: "xlink:arcrole",
	xlink_href: "xlink:href",
	xlink_role: "xlink:role",
	xlink_show: "xlink:show",
	xlink_title: "xlink:title",
	xlink_type: "xlink:type",
};

const SNAPSHOT_CACHE_TTL_MS = 12 * 60 * 60 * 1000;

interface CachedSnapshot {
	output: SvgCompatOutput;
	expiresAt: number;
}

const snapshotCache = new Map<string, CachedSnapshot>();
const snapshotInflight = new Map<string, Promise<SvgCompatSnapshot>>();
const RESPONSE_CACHE_CONTROL = "public, max-age=300, s-maxage=300, stale-while-revalidate=3600";
const SCHEMA_CACHE_CONTROL = "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400";

function canonicalAttributeName(name: string): string {
	return XLINK_MAP[name] ?? name;
}

export interface Baseline {
	status: "widely" | "newly" | "limited";
	since?: number;
	low_date?: string;
	high_date?: string;
}

export interface BrowserSupport {
	chrome?: string;
	edge?: string;
	firefox?: string;
	safari?: string;
}

export interface CompatEntry {
	description?: string;
	mdn_url?: string;
	deprecated: boolean;
	experimental: boolean;
	standard_track: boolean;
	spec_url: string[];
	baseline?: Baseline;
	browser_support?: BrowserSupport;
}

export interface AttributeEntry extends CompatEntry {
	elements: string[];
}

export interface SvgCompatOutput {
	generated_at: string;
	sources: SvgCompatSources;
	elements: Record<string, CompatEntry>;
	attributes: Record<string, AttributeEntry>;
}

interface SvgCompatSnapshot {
	sources: SvgCompatSources;
	elements: Record<string, CompatEntry>;
	attributes: Record<string, AttributeEntry>;
}

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

async function getOutput(url: URL): Promise<SvgCompatOutput> {
	const selection = parseSourceSelection(url);
	const key = `${selection.bcd}|${selection.wf}`;
	const cached = snapshotCache.get(key);
	if (cached && cached.expiresAt > Date.now()) return cached.output;

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

function escapeHtml(value: string): string {
	return value
		.replaceAll("&", "&amp;")
		.replaceAll("<", "&lt;")
		.replaceAll(">", "&gt;")
		.replaceAll("\"", "&quot;");
}

function formatBaseline(baseline: Baseline | undefined): string {
	if (!baseline) return "-";
	if (baseline.status === "limited") return "limited";
	return `${baseline.status} (${baseline.since ?? "?"})`;
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

function topElements(output: SvgCompatOutput): NamedCompatEntry[] {
	return Object.entries(output.elements)
		.map(([name, entry]) => ({ name, ...entry }))
		.slice(0, 24);
}

function topAttributes(output: SvgCompatOutput): NamedAttributeEntry[] {
	return Object.entries(output.attributes)
		.map(([name, entry]) => ({ name, ...entry }))
		.slice(0, 24);
}

function deprecatedEntries(output: SvgCompatOutput): NamedCompatEntry[] {
	return Object.entries(output.elements)
		.filter(([, entry]) => entry.deprecated)
		.map(([name, entry]) => ({ name, ...entry }))
		.slice(0, 12);
}

function limitedAttributes(output: SvgCompatOutput): NamedAttributeEntry[] {
	return Object.entries(output.attributes)
		.filter(([, entry]) => entry.baseline?.status === "limited")
		.map(([name, entry]) => ({ name, ...entry }))
		.slice(0, 12);
}

function renderCompatRows(entries: NamedCompatEntry[]): string {
	return entries
		.map((entry) => {
			const mdnCell = entry.mdn_url
				? `<a href="${escapeHtml(entry.mdn_url)}">MDN</a>`
				: "-";
			return `<tr><th scope="row"><code>${escapeHtml(entry.name)}</code></th><td>${
				escapeHtml(formatBaseline(entry.baseline))
			}</td><td>${escapeHtml(formatBrowserSupport(entry.browser_support))}</td><td>${mdnCell}</td></tr>`;
		})
		.join("");
}

function renderAttributeRows(entries: NamedAttributeEntry[]): string {
	return entries
		.map((entry) => {
			const mdnCell = entry.mdn_url
				? `<a href="${escapeHtml(entry.mdn_url)}">MDN</a>`
				: "-";
			const elements = entry.elements.length === 1 && entry.elements[0] === "*"
				? "global"
				: entry.elements.join(", ");
			return `<tr><th scope="row"><code>${escapeHtml(entry.name)}</code></th><td>${escapeHtml(elements)}</td><td>${
				escapeHtml(formatBaseline(entry.baseline))
			}</td><td>${mdnCell}</td></tr>`;
		})
		.join("");
}

function renderSourceRows(output: SvgCompatOutput): string {
	return Object.values(output.sources)
		.map((source) => {
			return `<tr><th scope="row"><code>${escapeHtml(source.package)}</code></th><td>${
				escapeHtml(source.requested)
			}</td><td>${escapeHtml(source.resolved)}</td><td>${escapeHtml(source.mode)}</td><td><a href="${
				escapeHtml(source.source_url)
			}">source</a></td></tr>`;
		})
		.join("");
}

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
  <style>
    :root { color-scheme: dark; }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: Inter, ui-sans-serif, system-ui, sans-serif;
      background: #0b1020;
      color: #e8ecf3;
      line-height: 1.5;
    }
    main {
      max-width: 1200px;
      margin: 0 auto;
      padding: 32px 20px 48px;
    }
    h1, h2, h3, p { margin-top: 0; }
    a { color: #8dd0ff; }
    .hero {
      padding: 24px;
      border: 1px solid #25304f;
      border-radius: 20px;
      background: linear-gradient(135deg, #131c34, #0e1427 60%, #1a2442);
      box-shadow: 0 24px 80px rgba(0, 0, 0, 0.28);
    }
    .stats {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 12px;
      margin: 24px 0;
    }
    .stat, section {
      border: 1px solid #25304f;
      border-radius: 16px;
      background: #10182d;
    }
    .stat {
      padding: 16px;
    }
    .stat strong {
      display: block;
      font-size: 1.8rem;
      margin-bottom: 4px;
    }
    .grid {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 16px;
    }
    section {
      padding: 20px;
      overflow-x: auto;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.92rem;
    }
    th, td {
      text-align: left;
      padding: 10px 0;
      border-top: 1px solid #25304f;
      vertical-align: top;
    }
    tr:first-child th, tr:first-child td {
      border-top: none;
    }
    code {
      font-family: "SFMono-Regular", Consolas, monospace;
      font-size: 0.9em;
      color: #b9f27c;
    }
    .muted {
      color: #99a6c3;
    }
    .pill {
      display: inline-block;
      padding: 4px 10px;
      border-radius: 999px;
      border: 1px solid #31406b;
      color: #b9c8ea;
      text-decoration: none;
      margin-right: 8px;
      margin-bottom: 8px;
    }
  </style>
</head>
<body>
  <main>
    <div class="hero">
      <p class="muted">SVG compatibility catalog</p>
      <h1>Browser face. Dynamic source knobs.</h1>
      <p class="muted">Generated ${
		escapeHtml(output.generated_at)
	}. Ask for <code>/data.json</code>, <code>?format=json</code>, or <code>Accept: application/json</code>. Override upstream packages with <code>?source=latest</code> or explicit <code>?bcd=&lt;version&gt;&amp;wf=&lt;version&gt;</code>.</p>
      <p>
        <a class="pill" href="${escapeHtml(jsonUrl)}">Open JSON endpoint</a>
        <a class="pill" href="${escapeHtml(schemaUrl)}">Open schema</a>
        <a class="pill" href="${escapeHtml(latestHtmlUrl)}">Try latest in browser</a>
        <a class="pill" href="${escapeHtml(latestJsonUrl)}">Try latest JSON</a>
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
        <h2>Element snapshot</h2>
        <p class="muted">First 24 sorted elements with baseline and browser floor.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Baseline</th><th>Support</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderCompatRows(topElements(output))}</tbody>
        </table>
      </section>
      <section>
        <h2>Attribute snapshot</h2>
        <p class="muted">First 24 sorted attributes with scope and baseline.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Elements</th><th>Baseline</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderAttributeRows(topAttributes(output))}</tbody>
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
  <style>
    body { margin: 0; font-family: Inter, ui-sans-serif, system-ui, sans-serif; background: #0b1020; color: #e8ecf3; }
    main { min-height: 100vh; display: grid; place-items: center; padding: 24px; }
    article { max-width: 720px; padding: 24px; border: 1px solid #25304f; border-radius: 16px; background: #10182d; }
    code { color: #b9f27c; }
  </style>
</head>
<body>
  <main>
    <article>
      <p><strong>${status}</strong></p>
      <h1>Request failed</h1>
      <p>${escapeHtml(message)}</p>
      <p>Ask for <code>application/json</code>, <code>/data.json</code>, or <code>text/html</code>.</p>
    </article>
  </main>
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

interface Server {
	fetch(request: Request): Promise<Response>;
}

const server: Server = {
	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);
		if (request.method !== "GET" && request.method !== "HEAD") {
			return new Response("Method not allowed", {
				status: 405,
				headers: { allow: "GET, HEAD" },
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
					headers: { "content-type": "text/plain; charset=utf-8" },
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
					"application/json; charset=utf-8",
					RESPONSE_CACHE_CONTROL,
					etag,
					lastModified,
				);
			}

			return cachedResponse(
				request,
				renderHtml(output, url),
				"text/html; charset=utf-8",
				RESPONSE_CACHE_CONTROL,
				etag,
				lastModified,
			);
		} catch (error) {
			const { status, message } = classifyError(error);
			if (wantsJson(request, url)) {
				return new Response(JSON.stringify({ error: { status, message } }, null, "  "), {
					status,
					headers: { "content-type": "application/json; charset=utf-8" },
				});
			}

			if (wantsHtml(request)) {
				return new Response(renderErrorHtml(status, message), {
					status,
					headers: { "content-type": "text/html; charset=utf-8" },
				});
			}

			return new Response(message, {
				status,
				headers: { "content-type": "text/plain; charset=utf-8" },
			});
		}
	},
} satisfies Deno.ServeDefaultExport;

export default server;
