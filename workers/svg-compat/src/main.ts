/**
 * SVG compat data server. The HTTP entry point — fetch handler,
 * routing, content negotiation, ETag/cache plumbing, and error
 * mapping. All data extraction logic lives in `./lib/mod.ts` and
 * is shared with the CLI in `./cli.ts`.
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
import {
	buildOutput,
	buildSnapshot,
	SVG_COMPAT_SCHEMA,
	type SvgCompatOutput,
	type SvgCompatSnapshot,
} from "./lib/mod.ts";
import { renderErrorHtml, renderHtml } from "./render.tsx";
import {
	InvalidSourceRequestError,
	loadSourceDataForSelection,
	parseSourceSelection,
	UpstreamSourceError,
} from "./sources.ts";

// Re-export the public lib surface so existing consumers
// (`view.ts`, `render.tsx`, `main_test.ts`) can continue to import
// from `./main.ts` without code churn.
export type {
	AttributeEntry,
	Baseline,
	BaselineDate,
	BrowserFlag,
	BrowserSupport,
	BrowserVersion,
	CompatEntry,
	SourceInfo,
	SvgCompatOutput,
	SvgCompatSnapshot,
	SvgCompatSources,
	VersionQualifier,
} from "./lib/mod.ts";
export { buildOutput, buildSnapshot, SVG_COMPAT_SCHEMA } from "./lib/mod.ts";

const snapshotInflight = new Map<string, Promise<SvgCompatSnapshot>>();
const JSON_INDENT = 2;
const PROD_GENERATED_AT = new Date(BOOT).toISOString();

const FAVICON_ICO_ASSET = loadStaticAsset("favicon.ico", MIME.ico);
const FAVICON_SVG_ASSET = loadStaticAsset("favicon.svg", MIME.svg);

function currentGeneratedAt(): string {
	return DEV ? new Date().toISOString() : PROD_GENERATED_AT;
}

/** Returns freshly-built output for the given request URL's source selection. */
async function getOutput(url: URL): Promise<SvgCompatOutput> {
	const selection = parseSourceSelection(url);
	const key = `${selection.bcd}|${selection.wf}`;

	if (!DEV) {
		const inflight = snapshotInflight.get(key);
		if (inflight) {
			const snapshot = await inflight;
			return buildOutput(snapshot, currentGeneratedAt());
		}
	}

	const promise = loadSourceDataForSelection(selection).then(buildSnapshot);

	if (!DEV) snapshotInflight.set(key, promise);
	try {
		const snapshot = await promise;
		return buildOutput(snapshot, currentGeneratedAt());
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
