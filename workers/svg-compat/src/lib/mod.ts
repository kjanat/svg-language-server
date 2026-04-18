/**
 * Public entry point for the svg-compat data extraction library.
 *
 * Both the worker HTTP server (`main.ts`) and the CLI tool
 * (`cli.ts`) consume this module — exactly one source of truth
 * for parsing BCD + web-features into the `SvgCompatOutput` shape.
 *
 * @module
 */

export * from "./build.ts";
export * from "./parse.ts";
export * from "./schema.ts";
export * from "./types.ts";
