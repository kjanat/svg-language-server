# svg-format

Structural formatter for SVG documents.

This crate formats SVG by parsing with tree-sitter and rebuilding a normalized
structure with tab indentation, deterministic attribute ordering, and stable
tag layout. Style/script text blocks are preserved without trying to parse and
rewrite CSS/JS semantics.

## API

```rust
use svg_format::{
    format,
    format_with_options,
    AttributeLayout,
    AttributeSort,
    FormatOptions,
    QuoteStyle,
    WrappedAttributeIndent,
};

let pretty = format(source);

let pretty_custom = format_with_options(
    source,
    FormatOptions {
        indent_width: 4,
        insert_spaces: false,
        max_inline_tag_width: 100,
        attribute_sort: AttributeSort::Canonical,
        attribute_layout: AttributeLayout::Auto,
        attributes_per_line: 1,
        space_before_self_close: true,
        quote_style: QuoteStyle::Preserve,
        wrapped_attribute_indent: WrappedAttributeIndent::OneLevel,
    },
);
```

## CLI

`svg-format` is also available as a CLI binary from the same crate.

```sh
# Format from stdin to stdout
cat icon.svg | cargo run -p svg-format -- --stdin

# Check whether a file would change
cargo run -p svg-format -- --check icon.svg

# Format file in place
cargo run -p svg-format -- --in-place icon.svg
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg-language-server
