import { assertEquals } from "@std/assert";
import server, { type SvgCompatOutput } from "./main.ts";

Deno.test("responds with JSON", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	assertEquals(res.headers.get("content-type"), "application/json");
	const data: SvgCompatOutput = await res.json();
	assertEquals(typeof data.generated_at, "string");
});

Deno.test("has elements", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.elements);
	assertEquals(names.length > 50, true, `expected 50+ elements, got ${names.length}`);
	assertEquals(names.includes("rect"), true);
	assertEquals(names.includes("svg"), true);
	assertEquals(names.includes("path"), true);
});

Deno.test("has attributes", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.attributes);
	assertEquals(names.length > 100, true, `expected 100+ attributes, got ${names.length}`);
	assertEquals(names.includes("fill"), true);
	assertEquals(names.includes("d"), true);
});

Deno.test("xlink attributes use colon notation", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	const data: SvgCompatOutput = await res.json();
	assertEquals("xlink:href" in data.attributes, true);
	assertEquals("xlink_href" in data.attributes, false);
});

Deno.test("element compat entry shape", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	const data: SvgCompatOutput = await res.json();
	const rect = data.elements.rect;
	assertEquals(rect !== undefined, true);
	assertEquals(typeof rect!.deprecated, "boolean");
	assertEquals(typeof rect!.experimental, "boolean");
});

Deno.test("attribute entry has elements list", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	const data: SvgCompatOutput = await res.json();
	const fill = data.attributes.fill;
	assertEquals(fill !== undefined, true);
	assertEquals(Array.isArray(fill!.elements), true);
	assertEquals(fill!.elements.length > 0, true);
});

Deno.test("elements are sorted", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.elements);
	const sorted = [...names].sort();
	assertEquals(names, sorted);
});

Deno.test("attributes are sorted", async () => {
	const res = await server.fetch(new Request("http://localhost/"));
	const data: SvgCompatOutput = await res.json();
	const names = Object.keys(data.attributes);
	const sorted = [...names].sort();
	assertEquals(names, sorted);
});
