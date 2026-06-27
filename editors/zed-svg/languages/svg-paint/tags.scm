; Paint url() id references for the injected svg_paint grammar (code nav).
; fill="url(#grad1)", stroke="url(#pattern)" — the host emits the id
; definitions; this emits the references from the injected paint tree.
((paint_server
  (iri_reference) @name) @reference.id
 (#match? @name "^#"))
