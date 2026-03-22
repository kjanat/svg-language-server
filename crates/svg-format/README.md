# svg-format

Structural formatter for SVG documents.

This crate formats SVG by parsing with tree-sitter and rebuilding a normalized
structure with tab indentation, deterministic attribute ordering, and stable
tag layout. Style/script text blocks are preserved without trying to parse and
rewrite CSS/JS semantics.

## API

```rust
use svg_format::{format, format_with_options, FormatOptions};

let pretty = format(source);

let pretty_tabs = format_with_options(
    source,
    FormatOptions {
        indent_width: 4,
        insert_spaces: false,
        max_inline_tag_width: 100,
    },
);
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
