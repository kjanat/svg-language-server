- [x] Fix mpath.href via curated layer
- [x] Audit for other spec gaps (39 missing edges found and fixed across all 4 snapshots)
- [x] Write crate data-provenance doc (SOURCES.md)
- [x] Add regression test for spec gaps (tests/spec_required_edges.rs, 16 tests)
- [x] Break circular dependency: build.rs now reads data/elements.json and augments
      per-snapshot profile attributes from the curated catalog; no more manual
      element_attribute_matrix.json injection needed for elements already in elements.json

## Remaining

- [ ] Extend data/attributes.json to cover all attrs referenced in elements.json
      (~60 attrs currently in elements.json are not in data/attributes.json, e.g.
      `dur`, `by`, `calcMode`, `method`, etc.)
      Until then: new attributes still require a one-time manual record injection
      into the relevant data/specs/*/attributes.json files.
