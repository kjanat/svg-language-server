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

- [ ] **MDN BCD per-file lookups (LSP runtime overlay):** `@mdn/browser-compat-data` ships
      individual JSON files per feature (e.g. `svg/elements/circle.json`), accessible via
      jsdelivr. The svg-compat worker already processes the full BCD bundle at build time;
      the LSP could additionally lazy-fetch individual per-feature files at runtime for
      fresher overrides, complementing the existing `RuntimeCompat` startup fetch. Any
      network access must remain opt-in so offline builds continue to work.
  - [ ] Eval. if we want to implement this in the first place.
