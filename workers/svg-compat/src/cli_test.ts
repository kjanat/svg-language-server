/**
 * Smoke test for the svg-compat CLI. Spawns `deno run -A src/cli.ts`
 * as a subprocess and checks that the output is the parseable JSON
 * we expect — proves end-to-end that the CLI consumes the same
 * `lib/mod.ts` exports the server uses.
 *
 * @module
 */

import { assertEquals, assertExists } from "@std/assert";

const CLI = ["run", "-A", "src/cli.ts"] as const;

async function runCli(args: string[]): Promise<{ code: number; stdout: string; stderr: string }> {
	const cmd = new Deno.Command("deno", {
		args: [...CLI, ...args],
		stdout: "piped",
		stderr: "piped",
	});
	const { code, stdout, stderr } = await cmd.output();
	return {
		code,
		stdout: new TextDecoder().decode(stdout),
		stderr: new TextDecoder().decode(stderr),
	};
}

Deno.test("cli emit schema produces parseable JSON with new baselineDate def", async () => {
	const { code, stdout } = await runCli(["emit", "schema"]);
	assertEquals(code, 0);
	const parsed = JSON.parse(stdout);
	assertEquals(parsed.title, "SVG Compat Output");
	assertExists(parsed.$defs.baselineDate);
	assertEquals(parsed.$defs.baselineDate.required, ["raw"]);
	// Sanity: the qualifier enum is the exact triple we agreed on.
	assertEquals(parsed.$defs.baselineDate.properties.qualifier.enum, [
		"before",
		"after",
		"approximately",
	]);
	// Baseline def now references the sub-object.
	assertEquals(
		parsed.$defs.baseline.properties.high_date,
		{ "$ref": "#/$defs/baselineDate" },
	);
});

Deno.test("cli emit data preserves ≤ qualifier on feGaussianBlur", async () => {
	const { code, stdout } = await runCli(["emit", "data"]);
	assertEquals(code, 0);
	const parsed = JSON.parse(stdout);
	const blur = parsed.elements.feGaussianBlur;
	assertExists(blur.baseline);
	assertEquals(blur.baseline.status, "widely");
	assertEquals(blur.baseline.since, 2021);
	assertEquals(blur.baseline.since_qualifier, "before");
	assertEquals(blur.baseline.high_date.raw, "≤2021-04-02");
	assertEquals(blur.baseline.high_date.date, "2021-04-02");
	assertEquals(blur.baseline.high_date.qualifier, "before");
});
