# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Shared `svg-tree` crate for tree-sitter traversal helpers
- Runtime compat overlay: diagnostics and completions now use live BCD data
  fetched at LSP startup, not just the compile-time catalog
- Re-lint open documents when runtime compat data arrives
- Suppression comments (`svg-lint-disable`, `svg-lint-disable-next-line`)
  with unused-suppression warnings
- Multiline tag suppression scope: a disable-next-line comment before a
  multiline opening tag suppresses diagnostics on all attribute lines
- `color-mix()` support in `oklch`, `oklab`, and `srgb` interpolation spaces
- CSS custom property (`var()`) resolution in embedded `<style>` blocks
- `DeprecatedElement`, `DeprecatedAttribute`, `ExperimentalElement`,
  `ExperimentalAttribute` diagnostic codes
- `MissingReferenceDefinition` diagnostic for `url(#id)` targets without
  a matching definition
- `textDocument/definition` for CSS class and custom property definitions
  across inline and external stylesheets
- External stylesheet resolution with `OnceLock` caching
- HTTP timeouts (30s compat, 10s stylesheets)
- Error logging for all previously silent failure paths (network, IO,
  parse, lock poison)
- Editor setup docs for VS Code, Neovim, and Zed
- Known limitations section in README

### Changed

- Split monolithic source files into focused crate modules
- Workspace-wide clippy lints: pedantic + nursery baseline, deny
  `unwrap`/`expect`, forbid `unsafe`
- Eliminated all `unwrap()`/`expect()` from production and test code
- Canonicalized xlink attribute names from underscore (`xlink_href`) to
  colon (`xlink:href`) form throughout the catalog
- Curated deprecation flags now preserved when BCD compat entry exists
  (`||` merge instead of replace)
- `parse_hue`/`parse_hue_f64` reject non-finite values (`NaN`, `Infinity`)
- `strip_suffix_ci` uses `str::get()` to avoid panic on UTF-8 boundaries
- `round_degrees_to_u16` swaps `rem_euclid`/`round` order to prevent
  359.6 from becoming 360
- `SpecUrl::first()` uses `urls.first().map_or()` to avoid panic on
  empty `Vec`
- Grammar load failures are now surfaced or safely fall back instead of
  silently returning empty results
- `parse_attribute` in formatter uses sequential `if let` instead of
  nested `map_or_else` closures
- `tag_parse` canonical ordering uses `u16::try_from(i).ok()` instead of
  `i as u16` with lint suppression

### Fixed

- `svg-format` now guarantees pure-LF output on all parse-error and
  ignore-file fallback paths, making the returned newline style part of
  the public contract. Previously `format_with_host` returned the source
  string verbatim (CRs included) whenever tree-sitter reported a parse
  error, language setup failed, or an `svg-format-ignore-file` directive
  was present. Downstream callers that translated line endings with a
  blanket `replace('\n', target)` — notably `dprint-plugin-svg` under
  auto-detected CRLF — silently doubled each `\r\n` into `\r\r\n`, causing
  dprint to bail with "Formatting not stable." A new `normalize_line_endings`
  helper is invoked before each early return.
- Hue wraparound: 359.6 degrees no longer produces `hsl(360, ...)`
- `var()` fallback: `var(--prop, red)` now tries the fallback when the
  property value is not a valid color
- Post-scaling infinity: `"1e38turn"` no longer produces `Infinity`
  after unit conversion
- `color_kinds` cache evicted on `did_close` to prevent stale entries
- `BrowserSupportValue` returns `None` when all browser fields are `None`,
  preventing overwrites during merge
