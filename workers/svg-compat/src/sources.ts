/**
 * Upstream data loading — resolves BCD and web-features packages from
 * bundled defaults or dynamically fetched versions via unpkg.
 *
 * @module
 */

import bcd from "@mdn/browser-compat-data" with { type: "json" };
import { features } from "web-features";

/** Loose JSON object type used to traverse BCD and web-features payloads without npm type imports. */
export interface JsonRecord {
	/** Any string key maps to an unknown value. */
	[key: string]: unknown;
}

/** Metadata about one upstream npm package used to build the response. */
export interface SourceInfo {
	/** npm package name (e.g. `@mdn/browser-compat-data`). */
	package: string;
	/** Version that was requested (from deno.json or query param). */
	requested: string;
	/** Version that was actually resolved after fetching. */
	resolved: string;
	/** Whether this used the bundled default or a dynamically fetched override. */
	mode: "default" | "override";
	/** URL the data was fetched from (or would be fetched from for defaults). */
	source_url: string;
}

/** The two upstream sources (BCD + web-features) with their resolved version info. */
export interface SvgCompatSources {
	/** MDN browser-compat-data source info. */
	bcd: SourceInfo;
	/** Web-features source info. */
	web_features: SourceInfo;
}

/** Requested package versions parsed from query params (`?bcd=...&wf=...` or `?source=latest`). */
export interface SourceSelection {
	/** Requested BCD version or dist-tag. */
	bcd: string;
	/** Requested web-features version or dist-tag. */
	wf: string;
}

/** Fully loaded and validated source payloads ready for processing. */
export interface LoadedSourceData {
	/** Full BCD root object. */
	bcdRoot: JsonRecord;
	/** The `bcd.svg` subtree. */
	svgRoot: JsonRecord;
	/** The `features` map from web-features. */
	featureMap: JsonRecord;
	/** Resolved source metadata for both packages. */
	sources: SvgCompatSources;
}

/** Thrown when query params contain invalid version tokens or source modes. Maps to HTTP 400. */
export class InvalidSourceRequestError extends Error {}

/** Thrown when an upstream fetch fails or returns unexpected data. Maps to HTTP 502. */
export class UpstreamSourceError extends Error {}

const VERSION_TOKEN = /^(latest|[A-Za-z0-9][A-Za-z0-9._-]*)$/;
const UPSTREAM_CACHE_TTL_MS = 12 * 60 * 60 * 1000;

interface CachedJsonRecord {
	body: JsonRecord;
	resolvedUrl: string;
	expiresAt: number;
}

const upstreamJsonCache = new Map<string, CachedJsonRecord>();
const upstreamJsonInflight = new Map<
	string,
	Promise<{ body: JsonRecord; resolvedUrl: string }>
>();

/**
 * Type guard for plain JSON objects.
 *
 * @param value Any value to check
 * @returns `true` if `value` is a non-null, non-array object
 */
export function isRecord(value: unknown): value is JsonRecord {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asRecord(value: unknown, message: string): JsonRecord {
	if (!isRecord(value)) throw new UpstreamSourceError(message);
	return value;
}

function getString(value: unknown): string | undefined {
	return typeof value === "string" ? value : undefined;
}

function resolvedVersion(specifier: string): string {
	const resolved = import.meta.resolve(specifier);
	const atMatch = resolved.match(/@(\d[^/]*)\//);
	if (atMatch?.[1]) return atMatch[1];
	const segments = new URL(resolved).pathname.split("/");
	const versionSegment = segments.find((s) => /^\d/.test(s));
	if (versionSegment) return versionSegment;
	throw new Error(`Cannot extract version from ${resolved}`);
}

function packageDataUrl(packageName: string, version: string): string {
	return `https://unpkg.com/${packageName}@${version}/data.json`;
}

/**
 * Extracts the resolved version from a URL like
 * `https://unpkg.com/@mdn/browser-compat-data@7.3.11/data.json`.
 *
 * @param location The full URL to extract from
 * @param packageName The npm package name (e.g. `@mdn/browser-compat-data`)
 * @returns The version string, or `undefined` if not found
 */
export function versionFromLocation(
	location: string,
	packageName: string,
): string | undefined {
	const url = new URL(location);
	const decodedPath = decodeURIComponent(url.pathname);
	const escapedPackageName = packageName.replaceAll("/", "\\/");
	const packageAtPattern = new RegExp(`/${escapedPackageName}@([^/]+)(?:/|$)`);
	const packageAtMatch = decodedPath.match(packageAtPattern);
	if (packageAtMatch?.[1]) return packageAtMatch[1];

	if (url.protocol === "https:") {
		const prefix = `/${packageName}@`;
		if (!url.pathname.startsWith(prefix)) return undefined;
		const version = url.pathname.slice(prefix.length).split("/")[0];
		return version.length > 0 ? version : undefined;
	}

	const segments = url.pathname
		.split("/")
		.filter((segment) => segment.length > 0);
	const packageSegments = packageName.split("/");
	for (
		let index = 0;
		index <= segments.length - packageSegments.length - 1;
		index++
	) {
		const matchesPackage = packageSegments.every(
			(segment, offset) => segments[index + offset] === segment,
		);
		if (!matchesPackage) continue;
		const version = segments[index + packageSegments.length];
		if (version) return version;
	}

	return undefined;
}

function parseVersionToken(
	name: string,
	value: string | null,
): string | undefined {
	if (value === null) return undefined;
	if (!VERSION_TOKEN.test(value)) {
		throw new InvalidSourceRequestError(
			`Invalid ${name} override. Use an exact version or dist-tag like latest.`,
		);
	}
	return value;
}

const DEFAULT_BCD_ROOT = asRecord(bcd, "Default BCD payload is not an object.");
const DEFAULT_BCD_META = isRecord(DEFAULT_BCD_ROOT.__meta)
	? DEFAULT_BCD_ROOT.__meta
	: undefined;
const DEFAULT_BCD_VERSION = getString(DEFAULT_BCD_META?.version)
	?? resolvedVersion("@mdn/browser-compat-data");

const DEFAULT_SVG_ROOT = asRecord(
	DEFAULT_BCD_ROOT.svg,
	"Default BCD payload is missing the svg root.",
);
const DEFAULT_FEATURE_MAP = asRecord(
	features,
	"Default web-features payload is not an object.",
);
const DEFAULT_WF_VERSION = resolvedVersion("web-features");

const DEFAULT_SOURCES: SvgCompatSources = {
	bcd: {
		package: "@mdn/browser-compat-data",
		requested: DEFAULT_BCD_VERSION,
		resolved: DEFAULT_BCD_VERSION,
		mode: "default",
		source_url: packageDataUrl("@mdn/browser-compat-data", DEFAULT_BCD_VERSION),
	},
	web_features: {
		package: "web-features",
		requested: DEFAULT_WF_VERSION,
		resolved: DEFAULT_WF_VERSION,
		mode: "default",
		source_url: packageDataUrl("web-features", DEFAULT_WF_VERSION),
	},
};

const DEFAULT_SOURCE_DATA: LoadedSourceData = {
	bcdRoot: DEFAULT_BCD_ROOT,
	svgRoot: DEFAULT_SVG_ROOT,
	featureMap: DEFAULT_FEATURE_MAP,
	sources: DEFAULT_SOURCES,
};

/**
 * Parses `?source=`, `?bcd=`, `?wf=` query params into a version selection.
 *
 * @param url The request URL to extract params from
 * @returns Resolved version selection
 * @throws {InvalidSourceRequestError} On invalid source mode or version token
 */
export function parseSourceSelection(url: URL): SourceSelection {
	const source = url.searchParams.get("source");
	if (source !== null && source !== "default" && source !== "latest") {
		throw new InvalidSourceRequestError(
			`Invalid source mode ${source}. Use source=default or source=latest.`,
		);
	}

	const requestedBcd = parseVersionToken("bcd", url.searchParams.get("bcd"));
	const requestedWf = parseVersionToken(
		"wf",
		url.searchParams.get("wf") ?? url.searchParams.get("web_features"),
	);
	const useLatest = source === "latest";

	return {
		bcd: requestedBcd ?? (useLatest ? "latest" : DEFAULT_BCD_VERSION),
		wf: requestedWf ?? (useLatest ? "latest" : DEFAULT_WF_VERSION),
	};
}

/**
 * Returns the pinned default versions from `deno.json` imports.
 *
 * @returns Selection using the bundled package versions
 */
export function defaultSourceSelection(): SourceSelection {
	return {
		bcd: DEFAULT_BCD_VERSION,
		wf: DEFAULT_WF_VERSION,
	};
}

function isDefaultSelection(selection: SourceSelection): boolean {
	return (
		selection.bcd === DEFAULT_BCD_VERSION
		&& selection.wf === DEFAULT_WF_VERSION
	);
}

async function fetchJsonRecord(
	url: string,
): Promise<{ body: JsonRecord; resolvedUrl: string }> {
	const cached = upstreamJsonCache.get(url);
	if (cached && cached.expiresAt > Date.now()) {
		return { body: cached.body, resolvedUrl: cached.resolvedUrl };
	}

	const inflight = upstreamJsonInflight.get(url);
	if (inflight) return await inflight;

	const promise = (async () => {
		const response = await fetch(url, {
			headers: { accept: "application/json" },
		});
		if (!response.ok) {
			throw new UpstreamSourceError(
				`Upstream fetch failed for ${url}: ${response.status}.`,
			);
		}

		const payload: unknown = await response.json();
		const result = {
			body: asRecord(payload, `Upstream payload from ${url} is not an object.`),
			resolvedUrl: response.url,
		};
		upstreamJsonCache.set(url, {
			...result,
			expiresAt: Date.now() + UPSTREAM_CACHE_TTL_MS,
		});
		return result;
	})();

	upstreamJsonInflight.set(url, promise);
	try {
		return await promise;
	} finally {
		upstreamJsonInflight.delete(url);
	}
}

async function loadBcdRoot(requested: string): Promise<{
	root: JsonRecord;
	svgRoot: JsonRecord;
	info: SourceInfo;
}> {
	if (requested === DEFAULT_BCD_VERSION) {
		return {
			root: DEFAULT_BCD_ROOT,
			svgRoot: DEFAULT_SVG_ROOT,
			info: DEFAULT_SOURCES.bcd,
		};
	}

	const requestedUrl = packageDataUrl("@mdn/browser-compat-data", requested);
	const { body, resolvedUrl } = await fetchJsonRecord(requestedUrl);
	const meta = isRecord(body.__meta) ? body.__meta : undefined;
	const resolved = getString(meta?.version)
		?? versionFromLocation(resolvedUrl, "@mdn/browser-compat-data")
		?? (requested === "latest" ? undefined : requested);
	if (!resolved) {
		throw new UpstreamSourceError("Could not resolve fetched BCD version.");
	}

	return {
		root: body,
		svgRoot: asRecord(body.svg, "Fetched BCD payload is missing the svg root."),
		info: {
			package: "@mdn/browser-compat-data",
			requested,
			resolved,
			mode: "override",
			source_url: resolvedUrl,
		},
	};
}

async function loadWebFeatures(requested: string): Promise<{
	featureMap: JsonRecord;
	info: SourceInfo;
}> {
	if (requested === DEFAULT_WF_VERSION) {
		return {
			featureMap: DEFAULT_FEATURE_MAP,
			info: DEFAULT_SOURCES.web_features,
		};
	}

	const requestedUrl = packageDataUrl("web-features", requested);
	const { body, resolvedUrl } = await fetchJsonRecord(requestedUrl);
	const featureMap = asRecord(
		body.features,
		"Fetched web-features payload is missing the features map.",
	);
	const resolved = versionFromLocation(resolvedUrl, "web-features")
		?? (requested === "latest" ? undefined : requested);
	if (!resolved) {
		throw new UpstreamSourceError(
			"Could not resolve fetched web-features version.",
		);
	}

	return {
		featureMap,
		info: {
			package: "web-features",
			requested,
			resolved,
			mode: "override",
			source_url: resolvedUrl,
		},
	};
}

/**
 * Loads BCD + web-features data for the given selection.
 * Uses bundled defaults when versions match, otherwise fetches from unpkg.
 *
 * @param selection Requested package versions
 * @returns Loaded and validated source payloads
 * @throws {UpstreamSourceError} If upstream fetch or validation fails
 */
export async function loadSourceDataForSelection(
	selection: SourceSelection,
): Promise<LoadedSourceData> {
	if (isDefaultSelection(selection)) return DEFAULT_SOURCE_DATA;

	const [bcdSource, webFeaturesSource] = await Promise.all([
		loadBcdRoot(selection.bcd),
		loadWebFeatures(selection.wf),
	]);

	return {
		bcdRoot: bcdSource.root,
		svgRoot: bcdSource.svgRoot,
		featureMap: webFeaturesSource.featureMap,
		sources: {
			bcd: bcdSource.info,
			web_features: webFeaturesSource.info,
		},
	};
}

/**
 * Convenience: parses source selection from URL and loads data.
 *
 * @param url The request URL with optional `?source=`, `?bcd=`, `?wf=` params
 * @returns Loaded and validated source payloads
 */
export async function loadSourceData(url: URL): Promise<LoadedSourceData> {
	return await loadSourceDataForSelection(parseSourceSelection(url));
}
