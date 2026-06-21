; SVG IDs are document-global — scope at root, not per-element
(svg_root_element) @local.scope

; id attribute definitions
((id_attribute
  value: (id_attribute_value
    (id_token) @local.definition)))

; href references to fragment IDs
((href_attribute
  value: (href_attribute_value
    (href_reference
      (iri_reference) @local.reference)))
 (#match? @local.reference "^#"))

; paint url() references
((paint_attribute
  value: (paint_attribute_value
    (paint_value
      (paint_server
        (iri_reference) @local.reference))))
 (#match? @local.reference "^#"))

; functional IRI references (clip-path, mask, filter, etc.)
((functional_iri_attribute
  value: (functional_iri_attribute_value
    (iri_reference) @local.reference))
 (#match? @local.reference "^#"))

((functional_iri_attribute
  value: (functional_iri_attribute_value
    (functional_iri
      (iri_reference) @local.reference)))
 (#match? @local.reference "^#"))
