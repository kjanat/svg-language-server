import assert from 'node:assert';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

/**
 * @param {string} packageRoot
 * @param {string} builtName
 * @param {string} prebuildName
 */
function ensureBunPrebuild(packageRoot, builtName, prebuildName) {
	const prebuildDir = join(packageRoot, 'prebuilds', `${process.platform}-${process.arch}`);
	const prebuildPath = join(prebuildDir, `${prebuildName}.node`);

	const builtPath = join(packageRoot, 'build', 'Release', builtName);
	assert.equal(existsSync(builtPath), true, `Missing runtime binding: ${builtPath}`);

	mkdirSync(prebuildDir, { recursive: true });
	copyFileSync(builtPath, prebuildPath);
}

function ensureBunPrebuilds() {
	if (typeof process.versions.bun !== 'string') {
		return;
	}

	const grammarRoot = fileURLToPath(new URL('../..', import.meta.url));
	ensureBunPrebuild(grammarRoot, 'tree_sitter_svg_paint_binding.node', 'tree-sitter-svg-paint');

	const require = createRequire(import.meta.url);
	const treeSitterPackageJson = require.resolve('tree-sitter/package.json');
	const treeSitterRoot = dirname(treeSitterPackageJson);
	ensureBunPrebuild(treeSitterRoot, 'tree_sitter_runtime_binding.node', 'tree-sitter');
}

test('can load grammar', async () => {
	ensureBunPrebuilds();

	const treeSitterModule = await import('tree-sitter');
	const Parser = treeSitterModule.default;
	const parser = new Parser();
	const { default: language } = await import('./index.js');
	parser.setLanguage(language);

	assert.equal(typeof language.HIGHLIGHTS_QUERY, 'string');
	assert.equal(typeof language.LOCALS_QUERY, 'string');
	assert.equal(typeof language.TAGS_QUERY, 'string');
	assert.equal(typeof language.TEXTOBJECTS_QUERY, 'string');
});
