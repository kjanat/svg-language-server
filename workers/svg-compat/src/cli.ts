#!/usr/bin/env -S deno run -A
/**
 * `svg-compat` — command-line entry for the SVG compat data
 * pipeline. Wraps the same `lib/mod.ts` exports the HTTP server
 * uses, so any consumer (CI fixtures, offline data dumps, the
 * Rust crates' build script) gets byte-identical output without
 * spinning up the worker.
 *
 * Usage:
 *   deno run -A src/cli.ts emit data
 *   deno run -A src/cli.ts emit data --bcd latest --wf latest
 *   deno run -A src/cli.ts emit data --out /tmp/svg-compat.json
 *   deno run -A src/cli.ts emit schema
 *   deno run -A src/cli.ts emit schema --out /tmp/svg-compat.schema.json
 *
 * @module
 */

import { cli, command, flag, group } from "@kjanat/dreamcli";

import { buildOutput, buildSnapshot, SVG_COMPAT_SCHEMA } from "./lib/mod.ts";
import { defaultSourceSelection, loadSourceDataForSelection } from "./sources.ts";

/**
 * Writes the given JSON text to a file or stdout, depending on
 * `outPath`. Status messages go to stderr so stdout stays clean
 * for shell pipes (`svg-compat emit data | jq ...`).
 */
async function emit(text: string, outPath: string | undefined): Promise<void> {
	if (outPath) {
		await Deno.writeTextFile(outPath, text + "\n");
		console.error(`svg-compat: wrote ${text.length} bytes to ${outPath}`);
		return;
	}
	console.log(text);
}

function formatJson(value: unknown, pretty: boolean): string {
	return pretty ? JSON.stringify(value, null, 2) : JSON.stringify(value);
}

// Resolved at module load so `--help` shows the real pinned
// versions ("7.3.11", "3.23.0") rather than an opaque sentinel.
const DEFAULTS = defaultSourceSelection();

const dataCommand = command("data")
	.description("Emit /data.json equivalent to stdout (or --out file).")
	.flag(
		"bcd",
		flag.string()
			.default(DEFAULTS.bcd)
			.describe("BCD version (e.g. '7.3.11') or 'latest'"),
	)
	.flag(
		"wf",
		flag.string()
			.default(DEFAULTS.wf)
			.describe("web-features version or 'latest'"),
	)
	.flag(
		"out",
		flag.string().describe("Write to file instead of stdout"),
	)
	.flag(
		"pretty",
		flag.boolean().default(true).describe("Pretty-print JSON output"),
	)
	.action(async ({ flags }) => {
		const sourceData = await loadSourceDataForSelection({
			bcd: flags.bcd,
			wf: flags.wf,
		});
		const snapshot = buildSnapshot(sourceData);
		const output = buildOutput(snapshot, new Date().toISOString());
		await emit(formatJson(output, flags.pretty), flags.out);
	});

const schemaCommand = command("schema")
	.description("Emit the JSON Schema for /data.json to stdout (or --out file).")
	.flag(
		"out",
		flag.string().describe("Write to file instead of stdout"),
	)
	.flag(
		"pretty",
		flag.boolean().default(true).describe("Pretty-print JSON output"),
	)
	.action(async ({ flags }) => {
		await emit(formatJson(SVG_COMPAT_SCHEMA, flags.pretty), flags.out);
	});

export const emitGroup = group("emit")
	.description("Emit data or schema as JSON")
	.command(dataCommand)
	.command(schemaCommand);

export const app = cli("svg-compat")
	.version("1.0.0")
	.description("Inspect and dump SVG browser compatibility data.")
	.command(emitGroup)
	.completions();

if (import.meta.main) {
	app.run();
}
