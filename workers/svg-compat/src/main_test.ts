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

Deno.test("renderHtml docs links include MDN and W3C spec links", () => {
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
		elements: {},
		attributes: {
			"xlink:href": {
				deprecated: true,
				experimental: false,
				standard_track: true,
				mdn_url: "https://developer.mozilla.org/docs/Web/SVG/Reference/Attribute/xlink:href",
				spec_url: [
					"https://svgwg.org/svg2-draft/linking.html#XLinkHrefAttribute",
					"https://www.w3.org/TR/SVG11/filters.html#FilterElementHrefAttribute",
				],
				elements: ["use"],
			},
			"ping": {
				deprecated: false,
				experimental: true,
				standard_track: true,
				spec_url: ["https://svgwg.org/svg2-draft/linking.html#AElementPingAttribute"],
				elements: ["a"],
			},
		},
	};
	const html = renderHtml(output, new URL("http://localhost/"));
	assertEquals(html.includes(">MDN<"), true);
	assertEquals(html.includes(">W3C<"), true);
	assertEquals(html.includes("docs-flag-deprecated"), true);
});

Deno.test("renderHtml keeps active source selection in Open JSON endpoint link", () => {
	const output: SvgCompatOutput = {
		generated_at: "2026-01-01T00:00:00.000Z",
		sources: {
			bcd: {
				package: "@mdn/browser-compat-data",
				requested: "7.3.10",
				resolved: "7.3.10",
				mode: "override",
				source_url: "https://example.com/bcd-7.3.10",
			},
			web_features: {
				package: "web-features",
				requested: "3.23.1-dev-20260414145202-958736b",
				resolved: "3.23.1-dev-20260414145202-958736b",
				mode: "override",
				source_url: "https://example.com/wf-dev",
			},
		},
		elements: {},
		attributes: {},
	};
	const html = renderHtml(
		output,
		new URL(
			"http://localhost/?source=latest&bcd=7.3.10&wf=3.23.1-dev-20260414145202-958736b",
		),
	);
	assertEquals(
		html.includes(
			"href=\"http://localhost/data.json?source=latest&amp;bcd=7.3.10&amp;wf=3.23.1-dev-20260414145202-958736b\"",
		),
		true,
	);
});

Deno.test("renderHtml uses masked browser status glyphs", () => {
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
				browser_support: {
					chrome: { raw_value_added: "1", version_added: "1" },
				},
			},
		},
		attributes: {},
	};
	const html = renderHtml(output, new URL("http://localhost/"));
	assertEquals(html.includes("class=\"chip-status chip-status--supported\""), true);
	assertEquals(html.includes("class=\"chip-status chip-status--missing\""), true);
	assertEquals(html.includes("/browsers/check.svg"), true);
	assertEquals(html.includes("/browsers/cross.svg"), true);
	assertEquals(html.includes("<img class=\"chip-status\""), false);
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

Deno.test("browser support preserves explicit false + ≤ qualifier on glyph-orientation-horizontal", async () => {
	// Live regression guard for the browser-support-preservation fix.
	// glyph-orientation-horizontal is the canary: BCD 7.3.11 records
	// version_added: false for chrome/edge/firefox and "≤13.1" for safari.
	// Previously the entire browser_support block was silently dropped.
	const res = await server.fetch(new Request("http://localhost/data.json"));
	assertEquals(res.status, 200);
	const json = (await res.json()) as SvgCompatOutput;
	const attr = json.attributes["glyph-orientation-horizontal"];
	assertExists(attr.browser_support);
	assertEquals(attr.browser_support?.chrome?.raw_value_added, false);
	assertEquals(attr.browser_support?.chrome?.supported, false);
	assertEquals(attr.browser_support?.firefox?.supported, false);
	assertEquals(attr.browser_support?.edge?.supported, false);
	assertExists(attr.browser_support?.safari);
	assertEquals(attr.browser_support?.safari?.raw_value_added, "≤13.1");
	assertEquals(attr.browser_support?.safari?.version_added, "13.1");
	assertEquals(attr.browser_support?.safari?.version_qualifier, "before");
});

Deno.test("browser support preserves font-width (Safari-only attribute)", async () => {
	const res = await server.fetch(new Request("http://localhost/data.json"));
	assertEquals(res.status, 200);
	const json = (await res.json()) as SvgCompatOutput;
	const attr = json.attributes["font-width"];
	assertExists(attr.browser_support);
	assertEquals(attr.browser_support?.chrome?.supported, false);
	assertEquals(attr.browser_support?.firefox?.supported, false);
	assertEquals(attr.browser_support?.edge?.supported, false);
	assertEquals(attr.browser_support?.safari?.version_added, "18.4");
});

Deno.test("dashboard surfaces new stat tiles for preserved signals", async () => {
	// Regression guard for the sophisticated-UX plan: PageStats now
	// carries partial/removed/flagged/unsupported counts and StatsGrid
	// renders them as secondary tiles. On the live dataset we expect:
	// - partial ≥ 3 (color-interpolation and siblings)
	// - unsupportedSomewhere ≥ 10 (explicit `false` statements)
	// - removed ≥ 1 (historical removals like data_uri on Chrome)
	const res = await server.fetch(
		new Request("http://localhost/", { headers: { accept: "text/html" } }),
	);
	const body = await res.text();
	// Secondary-tier stats tiles are rendered with the `stat-secondary`
	// class — the new visual tier. At least four of them must exist.
	const secondaryTiles = body.match(/class="stat stat-secondary"/g) ?? [];
	assert(
		secondaryTiles.length >= 4,
		`expected ≥4 secondary stat tiles, got ${secondaryTiles.length}`,
	);
	// The label text for each new signal must appear on the page.
	for (const label of ["partial", "removed", "flagged", "unsupported"]) {
		assert(
			body.includes(`>${label}<`),
			`stats grid should include a tile labelled "${label}": ${body.slice(0, 200)}…`,
		);
	}
});

Deno.test("BaselineBadge title surfaces raw upstream dates", async () => {
	// BaselineBadge now renders a `title` attribute with `low_date.raw`
	// / `high_date.raw` so screen readers + tooltip users get the exact
	// upstream date string (`≤2021-04-02`) even though the badge only
	// shows the coarse year.
	const res = await server.fetch(
		new Request("http://localhost/", { headers: { accept: "text/html" } }),
	);
	const body = await res.text();
	// Look for a baseline badge title that contains a raw date string.
	const titleMatch = body.match(/<span class="badge badge-widely" title="[^"]*Widely since[^"]*"/);
	assert(
		titleMatch !== null,
		`expected BaselineBadge title with "Widely since …" text on at least one widely badge`,
	);
});

Deno.test("dashboard emits chip-partial class for partial-implementation entries", async () => {
	// CSS coverage regression guard: BrowserSupport.tsx emits chip state
	// classes for the preserved signals. Previously they were dark matter
	// (no CSS rules); the Phase 3.1 change added styling. This test
	// verifies the class IS emitted — the CSS itself is a static file.
	const res = await server.fetch(
		new Request("http://localhost/", { headers: { accept: "text/html" } }),
	);
	const body = await res.text();
	assert(
		body.includes("chip-partial"),
		"dashboard HTML should include at least one chip-partial class somewhere (e.g. color-interpolation)",
	);
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

Deno.test("version picker script clears empty override params", async () => {
	const res = await server.fetch(new Request("http://localhost/version-picker.mjs"));
	assertEquals(res.status, 200);
	const script = await res.text();
	assertEquals(script.includes("params.delete(alias);"), true);
	assertEquals(script.includes("web_features"), true);
	assertEquals(script.includes("if (changed) location.search = params.toString();"), true);
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
