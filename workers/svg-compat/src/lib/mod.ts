/**
 * Public entry point for the svg-compat data extraction library.
 *
 * Both the worker HTTP server (`main.ts`) and the CLI tool
 * (`cli.ts`) consume this module — exactly one source of truth
 * for parsing BCD + web-features into the `SvgCompatOutput` shape.
 *
 * @module
 */
// @ts-nocheck Deno

export * from '#lib/build.ts';
export * from '#lib/parse.ts';
export * from '#lib/schema.ts';
export * from '#lib/types.ts';
