import { assertEquals, assertExists } from "@std/assert";
import server, { SVG_COMPAT_SCHEMA, type SvgCompatOutput } from "./main.ts";
import { defaultSourceSelection, parseSourceSelection, versionFromLocation } from "./sources.ts";

async function fetchJson(path = "/"): Promise<Response> {
	return await server.fetch(
		new Request(`http://localhost${path}`, {
			headers: { accept: "application/json" },
		}),
	);
}

Deno.test("responds with JSON", async () => {
	const res = await fetchJson();
	assertEquals(res.headers.get("content-type"), "application/json; charset=UTF-8");
	assertEquals(res.headers.get("cache-control")?.includes("max-age=300"), true);
	assertEquals(typeof res.headers.get("etag"), "string");
	assertEquals(typeof res.headers.get("last-modified"), "string");
	const data: SvgCompatOutput = await res.json();
	assertEquals(typeof data.generated_at, "string");
	assertEquals(typeof data.sources.bcd.resolved, "string");
	assertEquals(typeof data.sources.web_features.resolved, "string");
});

Deno.test("has elements", async () => {
	const res = await fetchJson();
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.elements);
	assertEquals(names.length > 50, true, `expected 50+ elements, got ${names.length}`);
	assertEquals(names.includes("rect"), true);
	assertEquals(names.includes("svg"), true);
	assertEquals(names.includes("path"), true);
});

Deno.test("has attributes", async () => {
	const res = await fetchJson();
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.attributes);
	assertEquals(names.length > 100, true, `expected 100+ attributes, got ${names.length}`);
	assertEquals(names.includes("fill"), true);
	assertEquals(names.includes("d"), true);
});

Deno.test("xlink attributes use colon notation", async () => {
	const res = await fetchJson();
	const data: SvgCompatOutput = await res.json();
	assertEquals("xlink:href" in data.attributes, true);
	assertEquals("xlink_href" in data.attributes, false);
});

Deno.test("element compat entry matches contract", async () => {
	const res = await fetchJson();
	const data: SvgCompatOutput = await res.json();
	const rect = data.elements.rect;
	assertExists(rect);
	assertEquals(typeof rect.deprecated, "boolean");
	assertEquals(typeof rect.experimental, "boolean");
	assertEquals(typeof rect.standard_track, "boolean");
	assertEquals(Array.isArray(rect.spec_url), true);
});

Deno.test("attribute entry has elements list", async () => {
	const res = await fetchJson();
	const data: SvgCompatOutput = await res.json();
	const fill = data.attributes.fill;
	assertExists(fill);
	assertEquals(Array.isArray(fill.elements), true);
	assertEquals(fill.elements.length > 0, true);
});

Deno.test("elements are sorted", async () => {
	const res = await fetchJson();
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.elements);
	const sorted = [...names].sort();
	assertEquals(names, sorted);
});

Deno.test("attributes are sorted", async () => {
	const res = await fetchJson();
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.attributes);
	const sorted = [...names].sort();
	assertEquals(names, sorted);
});

Deno.test("browser gets HTML explorer", async () => {
	const res = await server.fetch(
		new Request("http://localhost/", {
			headers: { accept: "text/html" },
		}),
	);
	assertEquals(res.headers.get("content-type"), "text/html; charset=UTF-8");
	const body = await res.text();
	assertEquals(body.includes("<title>SVG Compat</title>"), true);
	assertEquals(body.includes("Open JSON endpoint"), true);
	assertEquals(body.includes("Element snapshot"), true);
	assertEquals(body.includes("Upstream sources"), true);
});

Deno.test("non-browser requests without JSON accept get rejected", async () => {
	const res = await server.fetch(
		new Request("http://localhost/", {
			headers: { accept: "text/plain" },
		}),
	);
	assertEquals(res.status, 406);
});

Deno.test("data.json route returns JSON", async () => {
	const res = await server.fetch(new Request("http://localhost/data.json"));
	assertEquals(res.headers.get("content-type"), "application/json; charset=UTF-8");
	const data: SvgCompatOutput = await res.json();
	assertEquals(typeof data.generated_at, "string");
});

Deno.test("schema endpoint returns schema", async () => {
	const res = await server.fetch(new Request("http://localhost/schema.json"));
	assertEquals(res.headers.get("content-type"), "application/schema+json; charset=utf-8");
	assertEquals(res.headers.get("cache-control")?.includes("max-age=3600"), true);
	const schema = await res.json();
	assertEquals(schema.title, SVG_COMPAT_SCHEMA.title);
	assertEquals(schema.properties.generated_at.type, "string");
	assertEquals(schema.properties.sources.type, "object");
	assertEquals(schema.$defs.compatEntry.required.includes("spec_url"), true);
});

Deno.test("invalid source override fails", async () => {
	const res = await fetchJson("/?bcd=../../nope");
	assertEquals(res.status, 400);
	const body = await res.json();
	assertEquals(body.error.status, 400);
});

Deno.test("source selection defaults and latest shorthand", () => {
	const defaults = defaultSourceSelection();
	assertEquals(parseSourceSelection(new URL("http://localhost/")), defaults);
	assertEquals(parseSourceSelection(new URL("http://localhost/?source=latest")), {
		bcd: "latest",
		wf: "latest",
	});
	assertEquals(parseSourceSelection(new URL("http://localhost/?bcd=7.3.11&wf=3.23.0")), {
		bcd: "7.3.11",
		wf: "3.23.0",
	});
});

Deno.test("version inference handles bundled npm path", () => {
	const defaults = defaultSourceSelection();
	assertEquals(
		versionFromLocation(import.meta.resolve("npm:web-features"), "web-features"),
		defaults.wf,
	);
});

Deno.test("json endpoint returns 304 for matching etag", async () => {
	const first = await fetchJson();
	const etag = first.headers.get("etag");
	assertEquals(typeof etag, "string");
	const second = await server.fetch(
		new Request("http://localhost/data.json", {
			headers: {
				accept: "application/json",
				"if-none-match": etag ?? "",
			},
		}),
	);
	assertEquals(second.status, 304);
});

Deno.test("html endpoint returns 304 for matching etag", async () => {
	const first = await server.fetch(
		new Request("http://localhost/", {
			headers: { accept: "text/html" },
		}),
	);
	const etag = first.headers.get("etag");
	assertEquals(typeof etag, "string");
	const second = await server.fetch(
		new Request("http://localhost/", {
			headers: {
				accept: "text/html",
				"if-none-match": etag ?? "",
			},
		}),
	);
	assertEquals(second.status, 304);
});
