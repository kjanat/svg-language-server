/**
 * Drift guard: the svg_path grammar derives its command token from a hardcoded
 * letter set (constants.js) to stay standalone — no catalog JSON is pulled into
 * grammar.js. This test fails if that set ever diverges from the svg-data
 * catalog (tokens.path_command_letters) it mirrors.
 */

import { PATH_COMMAND_LETTERS } from '#constants';
import { describe, expect, test } from 'bun:test';
import { resolve } from 'node:path';

const catalogPath = resolve(import.meta.dir, '../../crates/svg-data/data/catalog.tree-sitter.json');
const tokens = (await Bun.file(catalogPath).json()).tokens as Record<string, string[]>;

const asSet = (xs: readonly string[]) => [...xs].sort();

describe('svg_path command letters mirror the svg-data catalog', () => {
	test('path_command_letters matches catalog tokens.path_command_letters', () => {
		expect(asSet(PATH_COMMAND_LETTERS)).toEqual(asSet(tokens.path_command_letters));
	});

	test('letter set is exactly the 10 commands in both cases', () => {
		expect(PATH_COMMAND_LETTERS).toHaveLength(20);
		for (const upper of ['M', 'Z', 'L', 'H', 'V', 'C', 'S', 'Q', 'T', 'A']) {
			expect(PATH_COMMAND_LETTERS).toContain(upper);
			expect(PATH_COMMAND_LETTERS).toContain(upper.toLowerCase());
		}
	});
});
