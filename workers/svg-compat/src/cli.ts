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
import { scanSvg2Spec } from "./spec_scan.ts";

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

/**
 * `scan-spec` — authoritative SVG 2 spec-source scanner.
 *
 * Reads a local checkout of the W3C svgwg repository and emits a
 * machine-readable JSON report of which elements/attributes/properties
 * SVG 2 defines, removes, or obsoletes. The output is the third signal
 * source for the Rust `reconcile_bcd_spec` build check.
 *
 * **Self-bootstrapping**: when `--svgwg-path` doesn't exist, the command
 * shallow-clones `https://github.com/w3c/svgwg.git` into that path
 * automatically (one-time, ~30s on first run, resumed across runs).
 * The clone is git-ignored at the repo root — it never enters the
 * project's history. The maintainer runs this once per spec bump to
 * regenerate `crates/svg-data/data/reviewed/spec_removals.json`.
 *
 * Pin the output via the commit hash recorded in `source_pin.commit` —
 * `git rev-parse HEAD` inside the clone is the source of truth.
 */
export const scanSpecCommand = command("scan-spec")
	.description("Scan a local svgwg checkout for SVG 2 feature removals.")
	.flag(
		"svgwg-path",
		flag.string()
			.default("./svgwg")
			.describe("Path to a local clone of https://github.com/w3c/svgwg (auto-cloned if missing)"),
	)
	.flag(
		"no-bootstrap",
		flag.boolean()
			.default(false)
			.describe("Fail instead of auto-cloning when the svgwg path is missing"),
	)
	.flag(
		"out",
		flag.string().describe("Also write pretty JSON to this file path"),
	)
	.action(async ({ flags, out }) => {
		const svgwgRoot = flags["svgwg-path"];
		await ensureSvgwgClone(out, svgwgRoot, flags["no-bootstrap"]);
		const { commit, commitDate } = await readSvgwgCommit(svgwgRoot);
		const report = await scanSvg2Spec({ svgwgRoot, commit, commitDate });

		if (flags.out) await writePrettyJsonFile(out, flags.out, report);

		if (out.jsonMode) {
			out.json(report);
			return;
		}

		out.log(`svg-compat spec-scan · ${report.source_pin.repository}@${commit.slice(0, 10)}`);
		out.log("");
		out.log(`  defined elements   : ${report.defined_elements.length}`);
		out.log(`  defined attributes : ${report.defined_attributes.length}`);
		out.log(`  defined properties : ${report.defined_properties.length}`);
		out.log(`  removed properties : ${report.removed_properties.length}`);
		out.log(`  obsoleted properties: ${report.obsoleted_properties.length}`);
		out.log(`  changelog removals : ${report.changelog_removals.length}`);
		if (report.removed_properties.length > 0) {
			out.log("");
			out.log("removed properties (from text.html `has been removed in SVG 2`):");
			for (const fact of report.removed_properties) {
				out.log(`  - ${fact.name}  [${fact.provenance.file}:${fact.provenance.line}]`);
			}
		}
		if (report.obsoleted_properties.length > 0) {
			out.log("");
			out.log("obsoleted properties (from text.html `has been obsoleted`):");
			for (const fact of report.obsoleted_properties) {
				out.log(`  - ${fact.name}  [${fact.provenance.file}:${fact.provenance.line}]`);
			}
		}
		out.log("");
		out.log("(pass --json or pipe stdout for the full JSON report)");
	});

const SVGWG_REMOTE = "https://github.com/w3c/svgwg.git";

/**
 * Ensure an svgwg working tree exists at `svgwgRoot`. If absent and
 * bootstrapping is allowed, shallow-clone from the W3C repo (depth 1
 * for speed — the scanner only reads `master/`, so deep history is
 * useless). If `noBootstrap` is set, fail with a clear instruction
 * instead.
 *
 * The clone path is git-ignored at the project root — it never enters
 * project history. Re-runs reuse the existing clone; the maintainer
 * is expected to `git pull` inside the clone manually when they want
 * a newer revision (rare, since spec bumps are infrequent).
 */
async function ensureSvgwgClone(
	out: Out,
	svgwgRoot: string,
	noBootstrap: boolean,
): Promise<void> {
	const masterDir = `${svgwgRoot.replace(/\/$/, "")}/master`;
	try {
		const stat = await Deno.stat(masterDir);
		if (stat.isDirectory) return;
	} catch (error) {
		if (!(error instanceof Deno.errors.NotFound)) throw error;
	}

	if (noBootstrap) {
		throw new Error(
			`svg-compat scan-spec: ${svgwgRoot} is missing and --no-bootstrap was set.\n`
				+ `Clone manually with:\n  git clone --depth=1 ${SVGWG_REMOTE} ${svgwgRoot}`,
		);
	}

	out.log(`svg-compat scan-spec: bootstrapping svgwg clone at ${svgwgRoot}`);
	out.log(`  git clone --depth=1 ${SVGWG_REMOTE} ${svgwgRoot}`);

	const cloneResult = await new Deno.Command("git", {
		args: ["clone", "--depth=1", SVGWG_REMOTE, svgwgRoot],
		stdout: "inherit",
		stderr: "inherit",
	}).output();
	if (!cloneResult.success) {
		throw new Error(
			`svg-compat scan-spec: failed to clone ${SVGWG_REMOTE} into ${svgwgRoot} `
				+ `(exit ${cloneResult.code}). Check network access or clone manually.`,
		);
	}
}

/**
 * Run `git rev-parse HEAD` + `git log -1` inside the svgwg checkout to
 * pin the scanner output to a specific commit. Fatal if git fails —
 * the output MUST carry a commit hash so future reviewers can map
 * findings back to the source revision.
 */
async function readSvgwgCommit(
	svgwgRoot: string,
): Promise<{ commit: string; commitDate: string | undefined }> {
	const commitResult = await new Deno.Command("git", {
		args: ["rev-parse", "HEAD"],
		cwd: svgwgRoot,
		stdout: "piped",
		stderr: "piped",
	}).output();
	if (!commitResult.success) {
		const stderr = new TextDecoder().decode(commitResult.stderr);
		throw new Error(
			`svg-compat scan-spec: failed to read HEAD in ${svgwgRoot}: ${stderr.trim()}`,
		);
	}
	const commit = new TextDecoder().decode(commitResult.stdout).trim();

	const dateResult = await new Deno.Command("git", {
		args: ["log", "-1", "--format=%cI", "HEAD"],
		cwd: svgwgRoot,
		stdout: "piped",
		stderr: "piped",
	}).output();
	const commitDate = dateResult.success
		? new TextDecoder().decode(dateResult.stdout).trim() || undefined
		: undefined;

	return { commit, commitDate };
}

export const emitGroup = group("emit")
	.description("Emit data or schema as JSON (or a human summary on a TTY)")
	.command(dataCommand)
	.command(schemaCommand);

export const app = cli("svg-compat")
	.version("1.0.0")
	.description("Inspect and dump SVG browser compatibility data.")
	.command(emitGroup)
	.command(scanSpecCommand)
	.completions();

if (import.meta.main) {
	const isTTY = Deno.stdout.isTerminal();
	app.run({ jsonMode: !isTTY, isTTY });
}
