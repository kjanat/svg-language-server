#!/usr/bin/env node

import { readFile, writeFile } from 'node:fs/promises';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

type CatalogTreeSitter = {
	schema_version: number;
	sources: unknown;
	attribute_buckets: Record<string, string[]>;
	tokens: Record<string, string[]>;
};

const scriptDir = dirname(fileURLToPath(import.meta.url));
const grammarRoot = resolve(scriptDir, '..');
const repoRoot = resolve(grammarRoot, '../..');
const sourcePath = resolve(repoRoot, 'crates/svg-data/data/catalog.tree-sitter.json');
const outPath = resolve(grammarRoot, 'grammar.json');

function displayPath(path: string): string {
	const relativePath = relative(repoRoot, path);
	if (relativePath === '') {
		return '.';
	}

	return relativePath.startsWith('..') ? path : relativePath;
}

const data = JSON.parse(await readFile(sourcePath, 'utf8')) as CatalogTreeSitter;

await writeFile(outPath, `${JSON.stringify(data, null, '\t')}\n`);

console.log(`grammar data written -> ${displayPath(outPath)}`);
console.log(`source -> ${displayPath(sourcePath)}`);
console.log('attribute buckets');
console.table(
	Object.entries(data.attribute_buckets).map(([name, values]) => ({
		name,
		count: values.length,
	})),
);
console.log('tokens');
console.table(
	Object.entries(data.tokens).map(([name, values]) => ({
		name,
		count: values.length,
	})),
);
