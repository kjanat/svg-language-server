/**
 * Tests for the svg-compat CLI. Split across two styles:
 *
 * - **Subprocess** (`Deno.Command`) — proves the real entry point
 *   (`import.meta.main` → `app.run(...)`) resolves defaults, loads
 *   source data, and writes bytes to the right streams. Subprocess
 *   stdout is always piped → auto-detected JSON mode.
 * - **In-process** (`runCommand()` from dreamcli testkit) — fine-
 *   grained control over `jsonMode` / `isTTY`, so we can assert
 *   human-mode rendering and `--json` override without spawning
 *   `deno` or needing a PTY.
 *
 * @module
 */

import { runCommand } from "@kjanat/dreamcli/testkit";
import { assertEquals, assertExists, assertStringIncludes } from "@std/assert";

import { dataCommand, schemaCommand } from "./cli.ts";

const CLI = ["run", "-A", "src/cli.ts"] as const;

interface SubprocessResult {
	code: number;
	stdout: string;
	stderr: string;
}

async function runCli(args: string[]): Promise<SubprocessResult> {
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

Deno.test("cli emit schema (piped) produces parseable JSON with new baselineDate def", async () => {
	const { code, stdout } = await runCli(["emit", "schema"]);
	assertEquals(code, 0);
	const parsed = JSON.parse(stdout);
	assertEquals(parsed.title, "SVG Compat Output");
	assertExists(parsed.$defs.baselineDate);
	assertEquals(parsed.$defs.baselineDate.required, ["raw"]);
	assertEquals(parsed.$defs.baselineDate.properties.qualifier.enum, [
		"before",
		"after",
		"approximately",
	]);
	assertEquals(
		parsed.$defs.baseline.properties.high_date,
		{ "$ref": "#/$defs/baselineDate" },
	);
});

Deno.test("cli emit data (piped) preserves ≤ qualifier on feGaussianBlur", async () => {
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

Deno.test("cli emit data --out writes pretty JSON and logs status to stderr", async () => {
	const outPath = await Deno.makeTempFile({ prefix: "svg-compat-", suffix: ".json" });
	try {
		const { code, stdout, stderr } = await runCli(["emit", "data", "--out", outPath]);
		assertEquals(code, 0);

		// Subprocess stdout is piped → auto jsonMode → minified JSON dump.
		const fromStdout = JSON.parse(stdout);
		assertExists(fromStdout.elements.feGaussianBlur);

		// `out.log()` in jsonMode routes to stderr — status line lands there.
		assertStringIncludes(stderr, `wrote `);
		assertStringIncludes(stderr, outPath);

		// File contents must be pretty-printed (2-space indent + trailing
		// newline) and structurally identical to the stdout dump.
		const fileBody = await Deno.readTextFile(outPath);
		assertEquals(fileBody.startsWith("{\n  \"generated_at\""), true);
		assertEquals(fileBody.endsWith("}\n"), true);
		const fromFile = JSON.parse(fileBody);
		assertEquals(fromFile.elements.feGaussianBlur, fromStdout.elements.feGaussianBlur);
	} finally {
		await Deno.remove(outPath);
	}
});

Deno.test("dataCommand (in-process, TTY mode) renders human summary, not JSON", async () => {
	const result = await runCommand(dataCommand, [], { isTTY: true, jsonMode: false });
	assertEquals(result.exitCode, 0);
	const stdout = result.stdout.join("");
	// Human summary markers — header + section labels + footer hint.
	assertStringIncludes(stdout, "svg-compat · generated ");
	assertStringIncludes(stdout, "sources");
	assertStringIncludes(stdout, "elements (baseline buckets)");
	assertStringIncludes(stdout, "attributes (baseline buckets)");
	assertStringIncludes(stdout, "(pass --json or pipe stdout for the full structured dump)");
	// No JSON dump bled through — stdout should not start with '{'.
	assertEquals(stdout.trimStart().startsWith("{"), false);
});

Deno.test("dataCommand (in-process, jsonMode) emits minified JSON to stdout", async () => {
	const result = await runCommand(dataCommand, [], { isTTY: false, jsonMode: true });
	assertEquals(result.exitCode, 0);
	const stdout = result.stdout.join("");
	// stdout is exactly one JSON document + trailing newline.
	const parsed = JSON.parse(stdout);
	assertExists(parsed.elements.feGaussianBlur);
	assertExists(parsed.sources.bcd);
});

Deno.test("schemaCommand (in-process, TTY mode) renders schema summary", async () => {
	const result = await runCommand(schemaCommand, [], { isTTY: true, jsonMode: false });
	assertEquals(result.exitCode, 0);
	const stdout = result.stdout.join("");
	assertStringIncludes(stdout, "svg-compat schema · SVG Compat Output");
	assertStringIncludes(stdout, "draft: https://json-schema.org/draft/2020-12/schema");
	assertStringIncludes(stdout, "top-level properties");
	assertStringIncludes(stdout, "generated_at");
	assertStringIncludes(stdout, "(pass --json or pipe stdout for the full schema dump)");
});

Deno.test("schemaCommand (in-process, jsonMode) emits the raw schema", async () => {
	const result = await runCommand(schemaCommand, [], { isTTY: false, jsonMode: true });
	assertEquals(result.exitCode, 0);
	const parsed = JSON.parse(result.stdout.join(""));
	assertEquals(parsed.title, "SVG Compat Output");
	assertExists(parsed.$defs.baselineDate);
});
