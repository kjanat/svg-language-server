# Discoveries

- Zed schema headers: in this repo, the working `$schema` URLs are the raw
  `kjanat/zed-editor/refs/heads/feature/extension-toml-schema/...` ones used by
  `zed-ldap`, not the guessed `zed-industries/zed/main/...` raw URLs.
- If Zed says no SVG language is registered after a grammar refactor, check the
  query files too: one stale node name in `languages/svg/highlights.scm`
  (`path_attribute_name` after the grammar renamed it to `d_attribute_name`)
  prevents the whole `SVG` language from loading.
- Zed's `jsx_tag_auto_close` matches exact grammar node names; SVG root tags
  must use the same `start_tag` / `end_tag` names as nested elements or
  top-level `<svg>` auto-close breaks even if queries still handle the root
  separately.
- Zed-side `languages/svg/locals.scm` may need to diverge from upstream: local
  defs capture bare `id` values (`foo`), while SVG refs are parsed as `#foo`, so
  Zed can use `#strip!` to normalize refs even though upstream `tree-sitter-svg`
  avoids that predicate for Helix compatibility.
- Grammar/query pin bumps need to track subtree shape changes, not just
  top-level node names.
  `tree-sitter-svg@8ef7d70591529b85624439337e1e9b3c38b47b14` changes
  functional-IRI values from
  `functional_iri_attribute_value -> paint_server -> iri_reference` to
  `functional_iri_attribute_value -> functional_iri -> iri_reference`; if
  `languages/svg/{tags,locals}.scm` are not updated in the same change, Zed ID
  navigation breaks for `clip-path`, `mask`, `filter`, `marker-*`, and `cursor`
  references.
- The same grammar pin intentionally rejects paint-style fallback tails on
  functional-IRI attributes, e.g. `clip-path="url(#clip) red"`. Treat that as an
  upstream parser strictness change, not something to preserve in Zed queries.
