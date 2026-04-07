# svg-lint

Structural linting for SVG documents — validates element nesting, attribute
usage, and ID uniqueness against the SVG spec.

## Rules

| Code                         | Severity | Description                                       |
| ---------------------------- | -------- | ------------------------------------------------- |
| `UnknownElement`             | Warning  | Element not in the SVG spec                       |
| `InvalidChild`               | Warning  | Child in void element or wrong-category nesting   |
| `DuplicateId`                | Warning  | Duplicate `id` attribute value                    |
| `UnknownAttribute`           | Warning  | Attribute not recognized for a given element      |
| `DeprecatedElement`          | Warning  | Element marked deprecated in the SVG/BCD catalog  |
| `DeprecatedAttribute`        | Warning  | Attribute marked deprecated (including `xlink:*`) |
| `ExperimentalElement`        | Hint     | Element marked experimental                       |
| `ExperimentalAttribute`      | Hint     | Attribute marked experimental                     |
| `MissingReferenceDefinition` | Warning  | `url(#id)` target has no matching definition      |
| `UnusedSuppression`          | Warning  | Suppression comment did not suppress anything     |

## API

```rust
use svg_lint::{lint, lint_tree, LintOverrides, CompatFlags};

// Parse and lint in one call
let diagnostics = lint(source);

// Lint an already-parsed tree
let diagnostics = lint_tree(source, &tree, None);

// Lint with runtime compat overrides (e.g. from live BCD data)
let overrides = LintOverrides {
    elements: [("font".into(), CompatFlags { deprecated: true, experimental: false })]
        .into_iter().collect(),
    attributes: Default::default(),
};
let diagnostics = lint_tree(source, &tree, Some(&overrides));
```

Each diagnostic includes a `DiagnosticCode`, `Severity`, message, and source
location (byte range + row/col).

## Suppression Comments

```xml
<!-- svg-lint-disable-next-line DuplicateId -->
<rect id="dup" />

<!-- svg-lint-disable MissingReferenceDefinition -->
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
