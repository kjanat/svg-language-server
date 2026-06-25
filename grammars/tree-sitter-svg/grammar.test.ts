import 'bun';
import {
	D_ATTRIBUTE_NAMES,
	GENERATED_ATTRIBUTE_BUCKET_KEYS,
	GRAMMAR_DEDICATED_ATTRIBUTE_NAMES,
	TOKEN_KEYS,
} from '#grammarFixtures';
import type { CatalogTreeSitter } from '#grammarMatchers';
import { bucketAttributeOverlaps, expectExactSetMatch, intersection, unionBucketAttributes } from '#grammarMatchers';
import { file, fileURLToPath } from 'bun';
import { describe, expect, test } from 'bun:test';
import { resolve } from 'node:path';

const repoRoot = resolve(import.meta.dir, '../..');
const catalogPath = resolve(repoRoot, 'crates/svg-data/data/catalog.tree-sitter.json');
const zedSvgQueryDir = resolve(repoRoot, 'editors/zed-svg/languages/svg');
const grammarJsonPath = fileURLToPath(import.meta.resolve('#grammarData'));
const grammarJsPath = fileURLToPath(import.meta.resolve('#grammar'));

async function loadCatalogTreeSitter(): Promise<CatalogTreeSitter> {
	return JSON.parse(await file(catalogPath).text()) as CatalogTreeSitter;
}

async function loadGrammarJson(): Promise<CatalogTreeSitter> {
	return JSON.parse(await file(grammarJsonPath).text()) as CatalogTreeSitter;
}

async function loadGrammarJs(): Promise<string> {
	return file(grammarJsPath).text();
}

describe('grammar.js matches catalog tree-sitter projection', () => {
	test('grammar.json is in sync with catalog.tree-sitter.json', async () => {
		const catalog = await loadCatalogTreeSitter();
		const grammarJson = await loadGrammarJson();
		expect(grammarJson).toEqual(catalog);
	});

	// Path-command projection moved to tree-sitter-svg-path (its grammar.test.ts
	// owns the path_command_letters drift guard); color-space / hue / angle token
	// projection moved to tree-sitter-svg-paint. The host no longer carries them,
	// so the corresponding assertions live in the sibling grammars' tests.

	test('d_attribute_name matches path_data bucket exactly', async () => {
		const catalog = await loadCatalogTreeSitter();

		expect(() => {
			expectExactSetMatch(
				[...D_ATTRIBUTE_NAMES],
				catalog.attribute_buckets.path_data,
				'd_attribute_name',
			);
		}).not.toThrow();
	});

	test('dedicated attribute names do not appear in generated attribute buckets', async () => {
		const catalog = await loadCatalogTreeSitter();
		const generated = unionBucketAttributes(catalog, GENERATED_ATTRIBUTE_BUCKET_KEYS);
		const overlap = intersection(generated, [...GRAMMAR_DEDICATED_ATTRIBUTE_NAMES]);

		expect(overlap).toEqual([]);
	});

	test('catalog attribute buckets are pairwise disjoint', async () => {
		const catalog = await loadCatalogTreeSitter();
		expect(bucketAttributeOverlaps(catalog)).toEqual([]);
	});

	test('grammar.js references every generated attribute bucket key', async () => {
		const grammarJs = await loadGrammarJs();

		for (const key of GENERATED_ATTRIBUTE_BUCKET_KEYS) {
			expect(grammarJs).toContain(`ATTRIBUTE_BUCKETS.${key}`);
		}
	});

	test('grammar.js references every host catalog token key', async () => {
		const grammarJs = await loadGrammarJs();

		for (const key of TOKEN_KEYS) {
			expect(grammarJs).toContain(`TOKENS.${key}`);
		}
	});

	test('NUMBER_PATTERN backs the number rule', async () => {
		const grammarJs = await loadGrammarJs();
		const matches = grammarJs.match(/token\(NUMBER_PATTERN\)/g) ?? [];

		expect(matches).toHaveLength(1);
	});

	test('Zed query copies stay in sync with grammar queries', async () => {
		for (const queryName of ['highlights.scm', 'injections.scm']) {
			const grammarQuery = await file(resolve(import.meta.dir, 'queries', queryName)).text();
			const zedQuery = await file(resolve(zedSvgQueryDir, queryName)).text();

			expect(zedQuery).toBe(grammarQuery);
		}
	});
});
