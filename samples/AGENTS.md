# SAMPLES KNOWLEDGE BASE

## OVERVIEW

Manual SVG fixtures for diagnostics, hover, completions, colors, and larger smoke-test documents. This directory is for examples and repro files, not automated test inputs.

## STRUCTURE

```text
samples/
├── diagnostics-*.svg    # intentionally broken/deprecated/experimental cases
├── hover-*.svg          # element, attribute, and compat hover probes
├── style-colors.svg     # embedded CSS color cases
├── color-swatches.svg   # color presentation examples
├── completions-test.svg # completion context probe
├── realistic-*.svg      # manual smoke-test documents
└── assets/              # external SVGs referenced by samples
```

## WHERE TO LOOK

| Task                                | Location                                 | Notes                                          |
| ----------------------------------- | ---------------------------------------- | ---------------------------------------------- |
| Repro diagnostics                   | `diagnostics-*.svg`                      | Includes intentional failures                  |
| Repro hover docs                    | `hover-*.svg`                            | Element, attribute, and compat cases           |
| Repro completion context            | `completions-test.svg`                   | SVG vs CSS completion boundaries               |
| Repro color extraction/presentation | `style-colors.svg`, `color-swatches.svg` | Inline CSS + direct paint values               |
| Repro linked asset behavior         | `assets/*.svg`                           | Support files for reference/image cases        |
| Manual smoke test larger files      | `realistic-*.svg`                        | Closer to real documents than minimal fixtures |

## CONVENTIONS

- File names track feature area, not source order.
- Samples may be intentionally invalid or awkward if that is the behavior under test.
- `assets/` files support other sample SVGs rather than standing alone.

## ANTI-PATTERNS

- Do not normalize or "fix" invalid samples unless the test intent changes.
- Do not use these files as formatting/style source.
- Do not assume sample coverage equals automated coverage.

## NOTES

- Rust tests do not currently read this directory directly.
