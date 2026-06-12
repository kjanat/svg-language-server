#!/usr/bin/env bun
/**
 * Re-vendor the W3C specification-API version histories that back the baked
 * `EDITION_INDEX`. Metadata only (versions/dates/status/uri) — no spec content —
 * so this is safe to run unattended: it cannot change which elements/attributes
 * are valid, only which published editions the crate knows about.
 *
 * For each input in `w3c-api/PROVENANCE.toml` it re-fetches `source_url`, writes
 * the raw bytes back, and rewrites the recorded `sha256`/`bytes` plus the
 * `[capture] date` so the provenance stays truthful (the build verifies these).
 *
 * Usage: `bun scripts/refresh-editions.ts`  (or `just refresh-editions`)
 */
import { error, log } from "node:console";
import { normalize } from "node:path";
import { exit, stdout } from "node:process";

interface Input {
	id: string;
	path: string;
	source_url: string;
}

interface Provenance {
	inputs?: Input[];
}

/** New provenance facts for one vendored file, keyed by its `path`. */
export type ProvenanceFacts = Map<string, { sha256: string; bytes: number }>;

async function main(): Promise<void> {
	const provenanceDir = normalize(`${import.meta.dir}/../crates/svg-data/data/sources/w3c-api`);
	const provenancePath = `${provenanceDir}/PROVENANCE.toml`;

	const source = await Bun.file(provenancePath).text();
	const parsed = Bun.TOML.parse(source) as Provenance;
	const inputs = parsed.inputs ?? [];
	if (inputs.length === 0) {
		error(`no [[inputs]] found in ${provenancePath}`);
		exit(1);
	}

	const facts: ProvenanceFacts = new Map();
	for (const input of inputs) {
		stdout.write(`fetching ${input.id} … `);
		const response = await fetch(input.source_url, {
			headers: { "User-Agent": "svg-language-server-refresh-editions" },
		});
		if (!response.ok) {
			error(`\nfetch ${input.source_url}: HTTP ${response.status}`);
			exit(1);
		}
		const bytes = new Uint8Array(await response.arrayBuffer());
		await Bun.write(`${provenanceDir}/${input.path}`, bytes);
		const sha256 = new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
		facts.set(input.path, { sha256, bytes: bytes.byteLength });
		log(`${bytes.byteLength} bytes  sha256=${sha256.slice(0, 12)}…`);
	}

	const today = new Date().toISOString().slice(0, 10);
	await Bun.write(provenancePath, rewriteProvenance(source, facts, today));

	log(`\nupdated ${provenancePath}`);
	log("next: cargo test -p svg-data   # repro tests gate the new index");
}

/**
 * Rewrite the per-input `sha256`/`bytes` lines and the `[capture] date` in place,
 * preserving comments and layout. A single forward pass tracks the current
 * `[[inputs]]` block (by its `path`, which precedes the hash lines) and the
 * `[capture]` table, replacing only the dynamic values.
 */
export function rewriteProvenance(
	src: string,
	byPath: ProvenanceFacts,
	captureDate: string,
): string {
	let currentPath: string | null = null;
	let inCapture = false;

	return src
		.split("\n")
		.map((line) => {
			const table = line.match(/^\s*\[([A-Za-z0-9_.[\]]+)\]/);
			if (table) {
				inCapture = table[1] === "capture";
				if (line.includes("[[inputs]]")) currentPath = null;
				return line;
			}

			const pathMatch = line.match(/^\s*path\s*=\s*"(.+)"\s*$/);
			if (pathMatch?.[1]) {
				currentPath = pathMatch[1];
				return line;
			}

			if (inCapture && /^\s*date\s*=/.test(line)) {
				return line.replace(/^(\s*date\s*=\s*).*/, `$1"${captureDate}"`);
			}

			if (currentPath) {
				const fact = byPath.get(currentPath);
				if (fact) {
					if (/^\s*sha256\s*=/.test(line)) {
						return line.replace(/=.*/, `= "${fact.sha256}"`);
					}
					if (/^\s*bytes\s*=/.test(line)) {
						return line.replace(/=.*/, `= ${fact.bytes}`);
					}
				}
			}
			return line;
		})
		.join("\n");
}

if (import.meta.main) {
	await main();
}
