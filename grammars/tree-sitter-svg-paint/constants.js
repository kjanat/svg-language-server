/**
 * @file Catalog-derived token sets for the svg_paint grammar.
 * @license MIT
 *
 * Hardcoded on purpose so the grammar stays standalone (no catalog JSON pulled
 * into grammar.js). `grammar.test.ts` asserts these stay equal to the svg-data
 * catalog (crates/svg-data/data/catalog.tree-sitter.json) — that test is the
 * drift guard. Imported by both grammar.js and the test.
 */

// @ts-check

export const COLOR_SPACES = /* dprint-ignore */ [
  'a98-rgb', 'display-p3', 'display-p3-linear', 'prophoto-rgb',
  'rec2020', 'rec2100-hlg', 'rec2100-linear', 'rec2100-pq',
  'srgb', 'srgb-linear',
  'xyz', 'xyz-d50', 'xyz-d65',
];

export const COLOR_INTERPOLATION_SPACES = /* dprint-ignore */ [
  'a98-rgb', 'display-p3', 'display-p3-linear',
  'hsl', 'hwb', 'lab', 'lch', 'oklab', 'oklch',
  'prophoto-rgb', 'rec2020',
  'srgb', 'srgb-linear',
  'xyz', 'xyz-d50', 'xyz-d65',
];

export const HUE_INTERPOLATION_METHODS = ['decreasing', 'increasing', 'longer', 'shorter'];

export const ANGLE_UNITS = ['deg', 'grad', 'rad', 'turn'];
