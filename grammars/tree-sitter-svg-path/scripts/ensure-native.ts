#!/usr/bin/env node
/**
 * Build this grammar's native addon and stage it where Bun's loader expects it:
 *
 *   prebuilds/<platform>-<arch>/tree-sitter-svg-path.node
 *
 * Node resolves `build/Release/*.node` through node-gyp-build; Bun's CommonJS
 * loader expects the prebuild layout. This bridges that gap. The shared
 * `tree-sitter` runtime addon is built by the host grammar's ensure-native (or
 * tree-sitter's own install), so this only builds the path sub-grammar's addon.
 */
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';

const require = createRequire(import.meta.url);
const platformDir = `${process.platform}-${process.arch}`;

function runNodeGyp(cwd: string) {
	const nodeGyp = require.resolve('node-gyp/bin/node-gyp.js');
	execFileSync(process.execPath, [nodeGyp, 'rebuild'], { cwd, stdio: 'inherit' });
}

function ensureAddon(pkgDir: string, builtName: string, prebuildName: string) {
	const built = join(pkgDir, 'build', 'Release', builtName);
	if (!existsSync(built)) {
		runNodeGyp(pkgDir);
	}
	if (!existsSync(built)) {
		throw new Error(`node-gyp did not produce ${built}`);
	}

	const prebuild = join(pkgDir, 'prebuilds', platformDir, `${prebuildName}.node`);
	mkdirSync(dirname(prebuild), { recursive: true });
	copyFileSync(built, prebuild);
}

try {
	const grammarRoot = resolve(import.meta.dirname!, '..');
	ensureAddon(grammarRoot, 'tree_sitter_svg_path_binding.node', 'tree-sitter-svg-path');
} catch (error) {
	const reason = error instanceof Error ? error.message : String(error);
	console.error(`ensure-native: could not build the native addon: ${reason}`);
	console.error(
		'A C toolchain and node-gyp are required (e.g. build-essential / Xcode CLT plus `node-gyp`).',
	);
	process.exit(1);
}
