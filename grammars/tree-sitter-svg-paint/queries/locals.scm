; Paint url() references for the injected svg_paint grammar.
; The reference lives in the injected tree; an LSP resolves it against the host
; document's id definitions (svg is one global id scope).
((paint_server
  (iri_reference) @local.reference)
 (#match? @local.reference "^#"))
