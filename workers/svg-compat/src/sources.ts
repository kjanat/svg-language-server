import bcd from "@mdn/browser-compat-data" with { type: "json" };
import { features } from "web-features";

export interface JsonRecord {
	[key: string]: unknown;
}

export interface SourceInfo {
	package: string;
	requested: string;
	resolved: string;
	mode: "default" | "override";
	source_url: string;
}

export interface SvgCompatSources {
	bcd: SourceInfo;
	web_features: SourceInfo;
}

export interface SourceSelection {
	bcd: string;
	wf: string;
}

export interface LoadedSourceData {
	bcdRoot: JsonRecord;
	svgRoot: JsonRecord;
	featureMap: JsonRecord;
	sources: SvgCompatSources;
}

export class InvalidSourceRequestError extends Error {}

export class UpstreamSourceError extends Error {}

const BCD_PACKAGE = "@mdn/browser-compat-data";
const WF_PACKAGE = "web-features";
const DEFAULT_BCD_REQUESTED_VERSION = "7.3.11";
const DEFAULT_WF_REQUESTED_VERSION = "3.23.0";
const VERSION_TOKEN = /^(latest|[A-Za-z0-9][A-Za-z0-9._-]*)$/;
const UPSTREAM_CACHE_TTL_MS = 12 * 60 * 60 * 1000;

interface CachedJsonRecord {
	body: JsonRecord;
	resolvedUrl: string;
	expiresAt: number;
}

const upstreamJsonCache = new Map<string, CachedJsonRecord>();
const upstreamJsonInflight = new Map<string, Promise<{ body: JsonRecord; resolvedUrl: string }>>();

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

function packageDataUrl(packageName: string, version: string): string {
	return `https://unpkg.com/${packageName}@${version}/data.json`;
}

export function versionFromLocation(location: string, packageName: string): string | undefined {
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

	const segments = url.pathname.split("/").filter((segment) => segment.length > 0);
	const packageSegments = packageName.split("/");
	for (let index = 0; index <= segments.length - packageSegments.length - 1; index++) {
		const matchesPackage = packageSegments.every(
			(segment, offset) => segments[index + offset] === segment,
		);
		if (!matchesPackage) continue;
		const version = segments[index + packageSegments.length];
		if (version) return version;
	}

	return undefined;
}

function parseVersionToken(name: string, value: string | null): string | undefined {
	if (value === null) return undefined;
	if (!VERSION_TOKEN.test(value)) {
		throw new InvalidSourceRequestError(
			`Invalid ${name} override. Use an exact version or dist-tag like latest.`,
		);
	}
	return value;
}

const DEFAULT_BCD_ROOT = asRecord(bcd, "Default BCD payload is not an object.");
const DEFAULT_BCD_META = isRecord(DEFAULT_BCD_ROOT.__meta) ? DEFAULT_BCD_ROOT.__meta : undefined;
const DEFAULT_BCD_VERSION = getString(DEFAULT_BCD_META?.version)
	?? versionFromLocation(import.meta.resolve(BCD_PACKAGE), BCD_PACKAGE)
	?? DEFAULT_BCD_REQUESTED_VERSION;

const DEFAULT_SVG_ROOT = asRecord(
	DEFAULT_BCD_ROOT.svg,
	"Default BCD payload is missing the svg root.",
);
const DEFAULT_FEATURE_MAP = asRecord(features, "Default web-features payload is not an object.");
const DEFAULT_WF_VERSION = versionFromLocation(import.meta.resolve(WF_PACKAGE), WF_PACKAGE)
	?? versionFromLocation(import.meta.resolve(`npm:${WF_PACKAGE}`), WF_PACKAGE)
	?? DEFAULT_WF_REQUESTED_VERSION;

const DEFAULT_SOURCES: SvgCompatSources = {
	bcd: {
		package: BCD_PACKAGE,
		requested: DEFAULT_BCD_REQUESTED_VERSION,
		resolved: DEFAULT_BCD_VERSION,
		mode: "default",
		source_url: packageDataUrl(BCD_PACKAGE, DEFAULT_BCD_VERSION),
	},
	web_features: {
		package: WF_PACKAGE,
		requested: DEFAULT_WF_REQUESTED_VERSION,
		resolved: DEFAULT_WF_VERSION,
		mode: "default",
		source_url: packageDataUrl(WF_PACKAGE, DEFAULT_WF_VERSION),
	},
};

const DEFAULT_SOURCE_DATA: LoadedSourceData = {
	bcdRoot: DEFAULT_BCD_ROOT,
	svgRoot: DEFAULT_SVG_ROOT,
	featureMap: DEFAULT_FEATURE_MAP,
	sources: DEFAULT_SOURCES,
};

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
		bcd: requestedBcd ?? (useLatest ? "latest" : DEFAULT_BCD_REQUESTED_VERSION),
		wf: requestedWf ?? (useLatest ? "latest" : DEFAULT_WF_REQUESTED_VERSION),
	};
}

export function defaultSourceSelection(): SourceSelection {
	return {
		bcd: DEFAULT_BCD_REQUESTED_VERSION,
		wf: DEFAULT_WF_REQUESTED_VERSION,
	};
}

function isDefaultSelection(selection: SourceSelection): boolean {
	return selection.bcd === DEFAULT_BCD_REQUESTED_VERSION && selection.wf === DEFAULT_WF_REQUESTED_VERSION;
}

async function fetchJsonRecord(url: string): Promise<{ body: JsonRecord; resolvedUrl: string }> {
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
			throw new UpstreamSourceError(`Upstream fetch failed for ${url}: ${response.status}.`);
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
	if (requested === DEFAULT_BCD_REQUESTED_VERSION) {
		return {
			root: DEFAULT_BCD_ROOT,
			svgRoot: DEFAULT_SVG_ROOT,
			info: DEFAULT_SOURCES.bcd,
		};
	}

	const requestedUrl = packageDataUrl(BCD_PACKAGE, requested);
	const { body, resolvedUrl } = await fetchJsonRecord(requestedUrl);
	const meta = isRecord(body.__meta) ? body.__meta : undefined;
	const resolved = getString(meta?.version)
		?? versionFromLocation(resolvedUrl, BCD_PACKAGE)
		?? (requested === "latest" ? undefined : requested);
	if (!resolved) {
		throw new UpstreamSourceError("Could not resolve fetched BCD version.");
	}

	return {
		root: body,
		svgRoot: asRecord(body.svg, "Fetched BCD payload is missing the svg root."),
		info: {
			package: BCD_PACKAGE,
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
	if (requested === DEFAULT_WF_REQUESTED_VERSION) {
		return {
			featureMap: DEFAULT_FEATURE_MAP,
			info: DEFAULT_SOURCES.web_features,
		};
	}

	const requestedUrl = packageDataUrl(WF_PACKAGE, requested);
	const { body, resolvedUrl } = await fetchJsonRecord(requestedUrl);
	const featureMap = asRecord(
		body.features,
		"Fetched web-features payload is missing the features map.",
	);
	const resolved = versionFromLocation(resolvedUrl, WF_PACKAGE) ?? (requested === "latest" ? undefined : requested);
	if (!resolved) {
		throw new UpstreamSourceError("Could not resolve fetched web-features version.");
	}

	return {
		featureMap,
		info: {
			package: WF_PACKAGE,
			requested,
			resolved,
			mode: "override",
			source_url: resolvedUrl,
		},
	};
}

export async function loadSourceDataForSelection(selection: SourceSelection): Promise<LoadedSourceData> {
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

export async function loadSourceData(url: URL): Promise<LoadedSourceData> {
	return await loadSourceDataForSelection(parseSourceSelection(url));
}
