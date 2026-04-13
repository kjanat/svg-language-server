import { assert, assertEquals, assertExists } from "@std/assert";
import server, { SVG_COMPAT_SCHEMA, type SvgCompatOutput } from "./main.ts";
import { renderHtml } from "./render.tsx";
import { defaultSourceSelection, parseSourceSelection, versionFromLocation } from "./sources.ts";

const DEV = (() => {
	try {
		return !Deno.env.get("DENO_DEPLOYMENT_ID");
	} catch {
		return true;
	}
})();

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
	assertEquals(res.headers.get("x-content-type-options"), "nosniff");
	assertEquals(res.headers.get("referrer-policy"), "strict-origin-when-cross-origin");
	if (DEV) {
		assertEquals(res.headers.get("cache-control"), "no-store");
	} else {
		assertEquals(res.headers.get("cache-control")?.includes("max-age=300"), true);
	}
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
	assertEquals(res.headers.get("x-content-type-options"), "nosniff");
	assertEquals(res.headers.get("referrer-policy"), "strict-origin-when-cross-origin");
	assertEquals(
		typeof res.headers.get("content-security-policy") === "string",
		true,
	);
	const body = await res.text();
	assertEquals(body.includes("<title>SVG Compat</title>"), true);
	assertEquals(body.includes("Open JSON endpoint"), true);
	assertEquals(body.includes("Browser face. Dynamic source knobs."), true);
	assertEquals(body.includes("Upstream sources"), true);
});

Deno.test("renderHtml includes baseline badge classes", () => {
	const output: SvgCompatOutput = {
		generated_at: "2026-01-01T00:00:00.000Z",
		sources: {
			bcd: {
				package: "@mdn/browser-compat-data",
				requested: "7.3.11",
				resolved: "7.3.11",
				mode: "default",
				source_url: "https://example.com/bcd",
			},
			web_features: {
				package: "web-features",
				requested: "3.23.0",
				resolved: "3.23.0",
				mode: "default",
				source_url: "https://example.com/wf",
			},
		},
		elements: {
			rect: {
				deprecated: false,
				experimental: false,
				standard_track: true,
				spec_url: [],
				baseline: { status: "widely", since: 2015 },
			},
			dialog: {
				deprecated: false,
				experimental: false,
				standard_track: true,
				spec_url: [],
				baseline: { status: "newly", since: 2024 },
			},
		},
		attributes: {
			fill: {
				deprecated: false,
				experimental: false,
				standard_track: true,
				spec_url: [],
				elements: ["*"],
				baseline: { status: "limited" },
			},
		},
	};
	const html = renderHtml(output, new URL("http://localhost/"));
	assertEquals(html.includes("class=\"badge badge-widely\""), true);
	assertEquals(html.includes("class=\"badge badge-newly\""), true);
	assertEquals(html.includes("class=\"badge badge-limited\""), true);
});

Deno.test("baseline parser preserves ≤ qualifier on real feGaussianBlur entry", async () => {
	// Live web-features dataset regression guard. feGaussianBlur is
	// the canary: baseline_high_date "≤2021-04-02" in v3.23.0. If
	// upstream rewrites this entry the test fails loudly and we revisit.
	const res = await server.fetch(new Request("http://localhost/data.json"));
	assertEquals(res.status, 200);
	const json = (await res.json()) as SvgCompatOutput;
	const blur = json.elements.feGaussianBlur;
	assertExists(blur.baseline);
	assertEquals(blur.baseline?.status, "widely");
	assertEquals(blur.baseline?.since, 2021);
	assertEquals(blur.baseline?.since_qualifier, "before");
	assertExists(blur.baseline?.high_date);
	assertEquals(blur.baseline?.high_date?.raw, "≤2021-04-02");
	assertEquals(blur.baseline?.high_date?.date, "2021-04-02");
	assertEquals(blur.baseline?.high_date?.qualifier, "before");
	assertExists(blur.baseline?.low_date);
	assertEquals(blur.baseline?.low_date?.raw, "≤2018-10-02");
	assertEquals(blur.baseline?.low_date?.date, "2018-10-02");
});

Deno.test("attributes table renders Support column", async () => {
	const res = await server.fetch(
		new Request("http://localhost/", { headers: { accept: "text/html" } }),
	);
	const body = await res.text();
	// Both the elements table and the attributes table must now have
	// a Support column header — Bug A regression guard.
	const supportHeaders = body.match(/<th[^>]*scope="col"[^>]*>Support<\/th>/g) ?? [];
	assert(
		supportHeaders.length >= 2,
		`expected ≥2 Support headers (elements + attributes), got ${supportHeaders.length}`,
	);
});

Deno.test("wildcard accept defaults to HTML", async () => {
	const res = await server.fetch(
		new Request("http://localhost/", {
			headers: { accept: "*/*" },
		}),
	);
	assertEquals(res.status, 200);
	assertEquals(res.headers.get("content-type"), "text/html; charset=UTF-8");
});

Deno.test("non-browser requests without JSON/HTML accept get rejected", async () => {
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
	if (DEV) {
		assertEquals(res.headers.get("cache-control"), "no-store");
	} else {
		assertEquals(res.headers.get("cache-control")?.includes("max-age=3600"), true);
	}
	const schema = await res.json();
	assertEquals(schema.title, SVG_COMPAT_SCHEMA.title);
	assertEquals(schema.properties.generated_at.type, "string");
	assertEquals(schema.properties.sources.type, "object");
	assertEquals(schema.$defs.compatEntry.required.includes("spec_url"), true);
});

Deno.test("favicon routes serve static assets", async () => {
	const ico = await server.fetch(new Request("http://localhost/favicon.ico"));
	assertEquals(ico.status, 200);
	assertEquals(ico.headers.get("x-content-type-options"), "nosniff");
	assertEquals(typeof ico.headers.get("etag"), "string");
	assertEquals(typeof ico.headers.get("last-modified"), "string");
	if (DEV) {
		assertEquals(ico.headers.get("cache-control"), "no-store");
	} else {
		assertEquals(ico.headers.get("cache-control")?.includes("immutable"), true);
	}

	const svg = await server.fetch(new Request("http://localhost/favicon.svg"));
	assertEquals(svg.status, 200);
	assertEquals(svg.headers.get("content-type"), "image/svg+xml");
});

Deno.test("root asset routes serve static assets", async () => {
	const css = await server.fetch(new Request("http://localhost/style.css"));
	assertEquals(css.status, 200);
	assertEquals(css.headers.get("x-content-type-options"), "nosniff");
	assertEquals(css.headers.get("content-type")?.startsWith("text/css"), true);
	await css.arrayBuffer();

	const js = await server.fetch(new Request("http://localhost/version-picker.mjs"));
	assertEquals(js.status, 200);
	assertEquals(js.headers.get("x-content-type-options"), "nosniff");
	assertEquals(js.headers.get("content-type")?.startsWith("text/javascript"), true);
	await js.arrayBuffer();

	const badge = await server.fetch(new Request("http://localhost/badges/baseline-newly.svg"));
	assertEquals(badge.status, 200);
	assertEquals(badge.headers.get("x-content-type-options"), "nosniff");
	assertEquals(badge.headers.get("content-type"), "image/svg+xml");
	await badge.arrayBuffer();
});

Deno.test("legacy /static asset routes redirect to root asset paths", async () => {
	const res = await server.fetch(new Request("http://localhost/static/style.css", { redirect: "manual" }));
	assertEquals(res.status, 308);
	assertEquals(res.headers.get("location"), "http://localhost/style.css");
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

Deno.test("version inference handles deploy npm resolver format", () => {
	assertEquals(
		versionFromLocation("npm:web-features@3.23.0", "web-features"),
		"3.23.0",
	);
	assertEquals(
		versionFromLocation("npm:@mdn/browser-compat-data@7.3.11", "@mdn/browser-compat-data"),
		"7.3.11",
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
	assertEquals(second.status, DEV ? 200 : 304);
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
	assertEquals(second.status, DEV ? 200 : 304);
});
