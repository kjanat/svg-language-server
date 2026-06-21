import assert from 'node:assert';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { test } from 'node:test';

function ensureBunTreeSitterPrebuild() {
	if (typeof process.versions.bun !== 'string') {
		return;
	}

	const require = createRequire(import.meta.url);
	const treeSitterPackageJson = require.resolve('tree-sitter/package.json');
	const treeSitterRoot = dirname(treeSitterPackageJson);
	const prebuildDir = join(treeSitterRoot, 'prebuilds', `${process.platform}-${process.arch}`);
	const prebuildPath = join(prebuildDir, 'tree-sitter.node');

	if (existsSync(prebuildPath)) {
		return;
	}

	const runtimeBinding = join(treeSitterRoot, 'build', 'Release', 'tree_sitter_runtime_binding.node');
	assert.equal(existsSync(runtimeBinding), true, `Missing runtime binding: ${runtimeBinding}`);

	mkdirSync(prebuildDir, { recursive: true });
	copyFileSync(runtimeBinding, prebuildPath);
}

test('can load grammar', async () => {
	ensureBunTreeSitterPrebuild();

	const treeSitterModule = await import('tree-sitter');
	const Parser = treeSitterModule.default;
	const parser = new Parser();
	const { default: language } = await import('./index.js');
	parser.setLanguage(language);

	assert.equal(typeof language.HIGHLIGHTS_QUERY, 'string');
	assert.equal(typeof language.INJECTIONS_QUERY, 'string');
	assert.equal(typeof language.LOCALS_QUERY, 'string');
	assert.equal(typeof language.TAGS_QUERY, 'string');
});
