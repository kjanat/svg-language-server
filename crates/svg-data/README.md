# svg-data

Compile-time SVG spec catalog generated from [MDN browser-compat-data] and [web-features].

All data is baked into the binary at build time — no runtime fetches or file I/O.

## API

```rust
// Look up a single element or attribute
let rect = svg_data::element("rect").unwrap();
let fill = svg_data::attribute("fill").unwrap();

// List all attributes applicable to an element
let attrs = svg_data::attributes_for("rect");

// List allowed children for a parent element
let children = svg_data::allowed_children("text");

// Iterate everything
for el in svg_data::elements() { /* ... */ }
for attr in svg_data::attributes() { /* ... */ }
```

Each entry includes description, MDN URL, deprecation flag, baseline browser support status, content model (for elements), and value types (for attributes).

## Part of [svg-language-server]

[MDN browser-compat-data]: https://github.com/mdn/browser-compat-data
[web-features]: https://github.com/nicedoc/web-features
[svg-language-server]: https://github.com/kjanat/svg-language-server

## TODO

- [ ] https://github.com/mdn/browser-compat-data/ look into loading shit from this repo directly.\
      Has per-attr/element whatever json files, that could be lazy loaded through e.g. jsdelivr.
