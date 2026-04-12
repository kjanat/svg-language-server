import { serveDir } from "@std/http";
import { contentType as mediaTypeFromExtension } from "@std/media-types";

function readDeploymentId(): string | undefined {
	try {
		return Deno.env.get("DENO_DEPLOYMENT_ID");
	} catch {
		return undefined;
	}
}

export const DEV = !readDeploymentId();
export const BOOT = Date.now();

interface CachePolicyOptions {
	maxAge: number;
	sharedMaxAge?: number;
	staleWhileRevalidate?: number;
	immutable?: boolean;
}

function buildCacheControl(options: CachePolicyOptions): string {
	const directives = [
		"public",
		`max-age=${options.maxAge}`,
		options.sharedMaxAge !== undefined ? `s-maxage=${options.sharedMaxAge}` : undefined,
		options.staleWhileRevalidate !== undefined
			? `stale-while-revalidate=${options.staleWhileRevalidate}`
			: undefined,
		options.immutable ? "immutable" : undefined,
	];
	return directives.filter((value): value is string => value !== undefined).join(", ");
}

function mediaTypeOrFallback(extension: string, fallback: string): string {
	return mediaTypeFromExtension(extension) ?? fallback;
}

export const CACHE_POLICY = {
	response: buildCacheControl({
		maxAge: 300,
		sharedMaxAge: 300,
		staleWhileRevalidate: 3600,
	}),
	schema: buildCacheControl({
		maxAge: 3600,
		sharedMaxAge: 3600,
		staleWhileRevalidate: 86400,
	}),
	staticAsset: buildCacheControl({
		maxAge: 31536000,
		immutable: true,
	}),
} as const;

export const MIME = {
	json: mediaTypeOrFallback(".json", "application/json; charset=utf-8"),
	html: mediaTypeOrFallback(".html", "text/html; charset=utf-8"),
	text: mediaTypeOrFallback(".txt", "text/plain; charset=utf-8"),
	schema: "application/schema+json; charset=utf-8",
	svg: mediaTypeOrFallback(".svg", "image/svg+xml"),
	ico: mediaTypeOrFallback(".ico", "image/x-icon"),
} as const;

const CSP_SCRIPT_SRC = DEV ? "script-src 'self' 'unsafe-inline'" : "script-src 'self'";
const CONTENT_SECURITY_POLICY = [
	"default-src 'self'",
	CSP_SCRIPT_SRC,
	"style-src 'self' 'unsafe-inline'",
	"img-src 'self' data:",
	"font-src 'self'",
	"connect-src 'self' https://data.jsdelivr.com",
	"object-src 'none'",
	"base-uri 'none'",
	"form-action 'none'",
	"frame-ancestors 'none'",
].join("; ");

export type NegotiatedFormat = "json" | "html" | "not-acceptable";

export interface StaticAsset {
	body: Uint8Array;
	contentType: string;
	etag: string;
	lastModified: string;
}

export function entityTag(value: string): string {
	return `W/"${value}"`;
}

export function hashString(value: string): string {
	let hash = 0x811c9dc5;
	for (let index = 0; index < value.length; index++) {
		hash ^= value.charCodeAt(index);
		hash = Math.imul(hash, 0x01000193);
	}
	return (hash >>> 0).toString(16);
}

export function requestMatchesCache(request: Request, etag: string, lastModified: string): boolean {
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

export function loadStaticAsset(fileName: string, fallbackContentType: string): StaticAsset {
	const url = new URL(`../static/${fileName}`, import.meta.url);
	const body = Deno.readFileSync(url);
	const stat = Deno.statSync(url);
	const mtime = stat.mtime?.getTime() ?? BOOT;
	return {
		body,
		contentType: mediaTypeFromExtension(`.${fileName.split(".").pop()}`) ?? fallbackContentType,
		etag: entityTag(`asset:${fileName}:${mtime}:${body.byteLength}`),
		lastModified: new Date(mtime).toUTCString(),
	};
}

export function applyCommonSecurityHeaders(headers: Headers): void {
	headers.set("x-content-type-options", "nosniff");
	headers.set("referrer-policy", "strict-origin-when-cross-origin");
}

export function applyHtmlSecurityHeaders(headers: Headers): void {
	applyCommonSecurityHeaders(headers);
	headers.set("content-security-policy", CONTENT_SECURITY_POLICY);
}

export function cachedResponse(
	request: Request,
	body: string,
	responseContentType: string,
	cacheControl: string,
	etag: string,
	lastModified: string,
	status = 200,
): Response {
	const responseCacheControl = DEV ? "no-store" : cacheControl;
	const headers = new Headers({
		"content-type": responseContentType,
		"cache-control": responseCacheControl,
		etag,
		"last-modified": lastModified,
		vary: "accept",
	});
	if (responseContentType.includes("text/html")) {
		applyHtmlSecurityHeaders(headers);
	} else {
		applyCommonSecurityHeaders(headers);
	}

	if (!DEV && requestMatchesCache(request, etag, lastModified)) {
		return new Response(null, { status: 304, headers });
	}
	return new Response(body, { status, headers });
}

export function staticAssetResponse(request: Request, asset: StaticAsset): Response {
	const headers = new Headers({
		"content-type": asset.contentType,
		"cache-control": DEV ? "no-store" : CACHE_POLICY.staticAsset,
		etag: asset.etag,
		"last-modified": asset.lastModified,
	});
	applyCommonSecurityHeaders(headers);
	if (!DEV && requestMatchesCache(request, asset.etag, asset.lastModified)) {
		return new Response(null, { status: 304, headers });
	}
	if (request.method === "HEAD") {
		return new Response(null, { headers });
	}
	const body = new Uint8Array(asset.body.byteLength);
	body.set(asset.body);
	return new Response(body, { headers });
}

export function textResponse(body: string, status = 200): Response {
	const headers = new Headers({
		"content-type": MIME.text,
		"cache-control": "no-store",
	});
	applyCommonSecurityHeaders(headers);
	return new Response(body, { status, headers });
}

export function jsonErrorResponse(status: number, message: string, jsonIndent = 2): Response {
	const headers = new Headers({
		"content-type": MIME.json,
		"cache-control": "no-store",
	});
	applyCommonSecurityHeaders(headers);
	return new Response(JSON.stringify({ error: { status, message } }, null, jsonIndent), {
		status,
		headers,
	});
}

export function wantsJson(request: Request, url: URL): boolean {
	if (url.pathname === "/data.json") return true;
	if (url.pathname === "/schema.json") return true;
	if (url.searchParams.get("format") === "json") return true;
	const accept = request.headers.get("accept");
	if (!accept) return false;
	return accept.includes("application/json") || accept.includes("application/*+json");
}

export function wantsHtml(request: Request): boolean {
	const accept = request.headers.get("accept");
	if (!accept) return true;
	return accept.includes("text/html") || accept.includes("*/*");
}

export function negotiateFormat(request: Request, url: URL): NegotiatedFormat {
	if (wantsJson(request, url)) return "json";
	if (wantsHtml(request)) return "html";
	return "not-acceptable";
}

export async function serveStaticRoute(request: Request, urlRoot = "static"): Promise<Response> {
	const response = await serveDir(request, {
		fsRoot: new URL("../static", import.meta.url).pathname,
		urlRoot,
	});
	applyCommonSecurityHeaders(response.headers);
	return response;
}
