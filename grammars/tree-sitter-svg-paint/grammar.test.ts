/**
 * Drift guard: the svg_paint grammar hardcodes the CSS color-space / hue /
 * angle token sets (constants.js) to stay standalone — no catalog JSON is
 * pulled into the grammar. This test fails if those copies ever diverge from
 * the svg-data catalog they mirror.
 */

import { ANGLE_UNITS, COLOR_INTERPOLATION_SPACES, COLOR_SPACES, HUE_INTERPOLATION_METHODS } from '#constants';
import { describe, expect, test } from 'bun:test';
import { resolve } from 'node:path';

const catalogPath = resolve(import.meta.dir, '../../crates/svg-data/data/catalog.tree-sitter.json');
const tokens = (await Bun.file(catalogPath).json()).tokens as Record<string, string[]>;

const asSet = (xs: readonly string[]) => [...xs].sort();

describe('svg_paint token sets mirror the svg-data catalog', () => {
	const cases: ReadonlyArray<[string, readonly string[], string]> = [
		['color_spaces', COLOR_SPACES, 'color_spaces'],
		['color_interpolation_spaces', COLOR_INTERPOLATION_SPACES, 'color_interpolation_spaces'],
		['hue_interpolation_methods', HUE_INTERPOLATION_METHODS, 'hue_interpolation_methods'],
		['angle_units', ANGLE_UNITS, 'angle_units'],
	];

	for (const [label, hardcoded, key] of cases) {
		test(`${label} matches catalog tokens.${key}`, () => {
			expect(asSet(hardcoded)).toEqual(asSet(tokens[key]));
		});
	}
});
