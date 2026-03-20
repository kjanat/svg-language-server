# svg-lint

Structural linting for SVG documents — validates element nesting, attribute usage, and ID uniqueness against the SVG spec.

## Rules

- **UnknownElement** — flags elements not in the SVG spec
- **InvalidChild** — detects children in void elements or wrong-category nesting
- **DuplicateId** — reports duplicate `id` attribute values
- **UnknownAttribute** — flags attributes not recognized for a given element

## API

```rust
use svg_lint::{lint, lint_tree};

// Parse and lint in one call
let diagnostics = lint(source);

// Or lint an already-parsed tree-sitter tree
let diagnostics = lint_tree(source, &tree);
```

Each diagnostic includes a `DiagnosticCode`, `Severity`, message, and source location.

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
