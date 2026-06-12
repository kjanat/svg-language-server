#!/usr/bin/env bun
/**
 * Re-vendor the svgwg editor's-draft sources at a new `master` commit.
 *
 * Unlike `refresh-editions` (metadata only), these files ARE spec content: a new
 * commit can change which elements/attributes/properties the derived catalog
 * considers valid. So the default is *vendor only* — fetch every input at the
 * new commit into a fresh `svgwg-<prefix>/` dir with correct sha256 + git_blob
 * provenance — and leave the active pin alone for a human to flip after
 * reviewing the rebuilt catalog diff.
 *
 * `--activate` additionally performs the mechanical, unique-string pin swaps
 * (build.rs dir const, snapshot.json + manifest commit/date) so the only
 * remaining human step is: rebuild, review the catalog diff, run the repro
 * tests, commit. The repro tests are the safety net that catches any
 * unintended catalog change.
 *
 * Usage:
 *   bun scripts/refresh-svgwg.ts <commit> [--activate]   (or `just refresh-svgwg <commit>`)
 */
import { error, log } from "node:console";
import { mkdir } from "node:fs/promises";
import { dirname, normalize } from "node:path";
import { argv, exit, stdout } from "node:process";

const repoRoot = normalize(`${import.meta.dirname}/..`);
const dataDir = `${repoRoot}/crates/svg-data/data`;
const snapshotPath = `${dataDir}/specs/Svg2EditorsDraft/snapshot.json`;

interface Pin {
	kind: string;
	commit?: string;
}
interface PinnedSource {
	pin: Pin;
}
interface Snapshot {
	pinned_sources?: PinnedSource[];
}
interface SvgwgInput {
	id: string;
	path: string;
	upstream: string;
}
interface SvgwgProvenance {
	inputs?: SvgwgInput[];
}

async function main(): Promise<void> {
	const newCommit = argv[2];
	if (!newCommit || !/^[0-9a-f]{40}$/.test(newCommit)) {
		error("usage: refresh-svgwg <40-hex-commit> [--activate]");
		exit(1);
	}
	const activate = argv.includes("--activate");

	const snapshot = (await Bun.file(snapshotPath).json()) as Snapshot;
	const oldCommit = snapshot.pinned_sources
		?.map((source) => source.pin)
		.find((pin) => pin.kind === "git_commit")?.commit;
	if (!oldCommit) {
		error(`no git_commit pin in ${snapshotPath}`);
		exit(1);
	}
	if (oldCommit === newCommit) {
		log(`already pinned to ${newCommit} — nothing to do`);
		exit(0);
	}

	const oldDir = `${dataDir}/sources/svgwg-${oldCommit.slice(0, 8)}`;
	const newDir = `${dataDir}/sources/svgwg-${newCommit.slice(0, 8)}`;
	const provText = await Bun.file(`${oldDir}/PROVENANCE.toml`).text();
	const inputs = (Bun.TOML.parse(provText) as SvgwgProvenance).inputs ?? [];
	if (inputs.length === 0) {
		error(`no [[inputs]] in ${oldDir}/PROVENANCE.toml`);
		exit(1);
	}

	// sha256 + git blob hash for one vendored file, keyed by its vendored `path`.
	const facts = new Map<string, { sha256: string; git_blob: string }>();
	for (const input of inputs) {
		const url = `https://raw.githubusercontent.com/w3c/svgwg/${newCommit}/${input.upstream}`;
		stdout.write(`fetching ${input.id} (${input.upstream}) … `);
		const response = await fetch(url, {
			headers: { "User-Agent": "svg-language-server-refresh-svgwg" },
		});
		if (!response.ok) {
			error(`\nfetch ${url}: HTTP ${response.status}`);
			exit(1);
		}
		const bytes = new Uint8Array(await response.arrayBuffer());
		const dest = `${newDir}/${input.path}`;
		await mkdir(dirname(dest), { recursive: true });
		await Bun.write(dest, bytes);
		facts.set(input.path, { sha256: sha256Hex(bytes), git_blob: gitBlobHex(bytes) });
		log(`${bytes.byteLength} bytes`);
	}

	const today = new Date().toISOString().slice(0, 10);
	await Bun.write(`${newDir}/PROVENANCE.toml`, rewriteSvgwgProvenance(provText, facts, oldCommit, newCommit, today));
	log(`\nvendored ${inputs.length} files into ${relativeToRoot(newDir)}`);

	if (activate) {
		await activatePin(oldCommit, newCommit, today);
		log("\nactivated new pin. next:");
		log("  cargo build -p svg-data         # regenerate the catalog");
		log("  cargo test  -p svg-data         # repro tests gate the change");
		log("  git diff -- crates/svg-data     # review the catalog delta, then commit");
	} else {
		log("\nvendor-only (no pin change). to activate, re-run with --activate, or edit:");
		// dprint-ignore
		log(`  crates/svg-data/build.rs                          svgwg-${oldCommit.slice(0, 8)} -> svgwg-${newCommit.slice(0, 8)}`);
		log(`  crates/svg-data/data/specs/Svg2EditorsDraft/snapshot.json   commit + date`);
		log(`  crates/svg-data/data/sources/svg2-ed-*.toml                 pin + locators + date`);
	}
}

function sha256Hex(bytes: Uint8Array): string {
	return new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
}

/** Git blob object hash: `sha1("blob " + len + "\0" + content)`. */
function gitBlobHex(bytes: Uint8Array): string {
	const header = new TextEncoder().encode(`blob ${bytes.byteLength}\0`);
	const framed = new Uint8Array(header.byteLength + bytes.byteLength);
	framed.set(header, 0);
	framed.set(bytes, header.byteLength);
	return new Bun.CryptoHasher("sha1").update(framed).digest("hex");
}

function relativeToRoot(absolute: string): string {
	return absolute.startsWith(`${repoRoot}/`) ? absolute.slice(repoRoot.length + 1) : absolute;
}

/**
 * Produce the new dir's PROVENANCE.toml: swap every occurrence of the old commit
 * for the new one (covers `[pin] value` and prose references), update the
 * `[pin] date`, and replace each input's `git_blob`/`sha256` by `path`.
 */
export function rewriteSvgwgProvenance(
	src: string,
	byPath: Map<string, { sha256: string; git_blob: string }>,
	oldCommit: string,
	newCommit: string,
	pinDate: string,
): string {
	let currentPath: string | null = null;
	let inPin = false;

	return src
		.split(oldCommit)
		.join(newCommit)
		.split("\n")
		.map((line) => {
			const table = line.match(/^\s*\[([A-Za-z0-9_.[\]]+)\]/);
			if (table) {
				inPin = table[1] === "pin";
				if (line.includes("[[inputs]]")) currentPath = null;
				return line;
			}
			const pathMatch = line.match(/^\s*path\s*=\s*"(.+)"\s*$/);
			if (pathMatch?.[1]) {
				currentPath = pathMatch[1];
				return line;
			}
			if (inPin && /^\s*date\s*=/.test(line)) {
				return line.replace(/^(\s*date\s*=\s*).*/, `$1"${pinDate}"`);
			}
			if (currentPath) {
				const fact = byPath.get(currentPath);
				if (fact) {
					if (/^\s*git_blob\s*=/.test(line)) return line.replace(/=.*/, `= "${fact.git_blob}"`);
					if (/^\s*sha256\s*=/.test(line)) return line.replace(/=.*/, `= "${fact.sha256}"`);
				}
			}
			return line;
		})
		.join("\n");
}

/** Flip the active pin: build.rs dir const + rerun line, snapshot.json, manifest. */
async function activatePin(from: string, to: string, date: string): Promise<void> {
	const oldPrefix = `svgwg-${from.slice(0, 8)}`;
	const newPrefix = `svgwg-${to.slice(0, 8)}`;

	const buildRsPath = `${repoRoot}/crates/svg-data/build.rs`;
	const buildRs = await Bun.file(buildRsPath).text();
	await Bun.write(buildRsPath, buildRs.split(oldPrefix).join(newPrefix));

	// snapshot.json + manifest: the commit is a unique 40-hex string, so a global
	// swap is safe and preserves formatting. Then bump their capture dates.
	for (const path of [snapshotPath, ...(await manifestPaths())]) {
		const text = await Bun.file(path).text();
		const swapped = text.split(from).join(to).replace(/("date"\s*:\s*")[\d-]+(")/g, `$1${date}$2`).replace(
			/(\bdate\s*=\s*")[\d-]+(")/g,
			`$1${date}$2`,
		);
		await Bun.write(path, swapped);
	}
}

/** Editor's-draft source manifests under data/sources (svg2-ed-*.toml). */
async function manifestPaths(): Promise<string[]> {
	const glob = new Bun.Glob("svg2-ed-*.toml");
	const dir = `${dataDir}/sources`;
	const out: string[] = [];
	for await (const name of glob.scan({ cwd: dir })) out.push(`${dir}/${name}`);
	return out;
}

if (import.meta.main) {
	await main();
}
