#!/usr/bin/env -S deno run -A
/**
 * `svg-compat` — command-line entry for the SVG compat data pipeline.
 *
 * Wraps the same `lib/mod.ts` exports the HTTP server uses, so any consumer
 * (CI fixtures, offline data dumps, the Rust crates' build script) gets
 * byte-identical output without spinning up the worker.
 *
 * Dual-output shape:
 *
 * - Interactive TTY          → human summary tables via `cli_render.ts`.
 * - Piped / non-TTY stdout   → auto-switch to minified JSON dump (`out.json`).
 * - Explicit `--json`        → force JSON dump even on a TTY.
 * - `--out <file>`           → pretty JSON written to disk, status line logged.
 *
 * Pipe-detection happens in `if (import.meta.main)` below — dreamcli
 * itself has no TTY awareness, so we pass `jsonMode` / `isTTY` into
 * `app.run()` based on `Deno.stdout.isTerminal()`.
 *
 * Usage:
 *   deno run -A src/cli.ts emit data
 *   deno run -A src/cli.ts emit data --bcd latest --wf latest
 *   deno run -A src/cli.ts emit data --out /tmp/svg-compat.json
 *   deno run -A src/cli.ts emit data --json
 *   deno run -A src/cli.ts emit schema
 *
 * @module
 */

import { cli, command, flag, group, type Out } from "@kjanat/dreamcli";

import { renderDataSummary, renderSchemaSummary } from "./cli_render.ts";
import { buildOutput, buildSnapshot, SVG_COMPAT_SCHEMA } from "./lib/mod.ts";
import { defaultSourceSelection, loadSourceDataForSelection } from "./sources.ts";

const FILE_INDENT = 2;

/**
 * Writes `value` as pretty JSON to `path` and logs a status line
 * through the dreamcli channel. In JSON mode, `out.log()` auto-
 * routes to stderr so stdout stays pure-JSON for pipes; in human
 * mode it goes to stdout alongside the summary, which is the
 * expected behavior when a person is watching the terminal.
 */
async function writePrettyJsonFile(
	out: Out,
	path: string,
	value: unknown,
): Promise<void> {
	const body = JSON.stringify(value, null, FILE_INDENT) + "\n";
	await Deno.writeTextFile(path, body);
	out.log(`svg-compat: wrote ${body.length} bytes to ${path}`);
}

// Resolved at module load so `--help` shows the real pinned versions
// (e.g. "7.3.11", "3.23.0") rather than an opaque sentinel.
const DEFAULTS = defaultSourceSelection();

export const dataCommand = command("data")
	.description("Emit /data.json equivalent — summary on TTY, JSON when piped.")
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
		flag.string().describe("Also write pretty JSON to this file path"),
	)
	.action(async ({ flags, out }) => {
		const sourceData = await loadSourceDataForSelection({
			bcd: flags.bcd,
			wf: flags.wf,
		});
		const snapshot = buildSnapshot(sourceData);
		const output = buildOutput(snapshot, new Date().toISOString());

		if (flags.out) await writePrettyJsonFile(out, flags.out, output);

		if (out.jsonMode) {
			out.json(output);
			return;
		}

		renderDataSummary(out, output);
	});

export const schemaCommand = command("schema")
	.description("Emit the /data.json JSON Schema — summary on TTY, JSON when piped.")
	.flag(
		"out",
		flag.string().describe("Also write pretty JSON to this file path"),
	)
	.action(async ({ flags, out }) => {
		if (flags.out) await writePrettyJsonFile(out, flags.out, SVG_COMPAT_SCHEMA);

		if (out.jsonMode) {
			out.json(SVG_COMPAT_SCHEMA);
			return;
		}

		renderSchemaSummary(out, SVG_COMPAT_SCHEMA);
	});

export const emitGroup = group("emit")
	.description("Emit data or schema as JSON (or a human summary on a TTY)")
	.command(dataCommand)
	.command(schemaCommand);

export const app = cli("svg-compat")
	.version("1.0.0")
	.description("Inspect and dump SVG browser compatibility data.")
	.command(emitGroup)
	.completions();

if (import.meta.main) {
	const isTTY = Deno.stdout.isTerminal();
	app.run({ jsonMode: !isTTY, isTTY });
}
