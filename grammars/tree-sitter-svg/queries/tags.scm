; ─── ID definitions with docstrings ──────────────────────────────
; Comments adjacent to id-bearing elements serve as documentation.
; These patterns must precede the simple id_attribute fallback
; because tree-sitter tags uses the first matching pattern.
;
; Two variants per element form: with/without intervening (text) node.
; Whitespace between comment and element produces a (text) sibling;
; inline placement (<!-- doc --><el/>) does not.

; with whitespace — start_tag
(
  (comment (comment_text) @doc)
  .
  (text)
  .
  (element
    (start_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; with whitespace — self_closing_tag
(
  (comment (comment_text) @doc)
  .
  (text)
  .
  (element
    (self_closing_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; without whitespace — start_tag
(
  (comment (comment_text) @doc)
  .
  (element
    (start_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; without whitespace — self_closing_tag
(
  (comment (comment_text) @doc)
  .
  (element
    (self_closing_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; with whitespace — svg_root_element start_tag
(
  (comment (comment_text) @doc)
  .
  (text)
  .
  (svg_root_element
    (start_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; with whitespace — svg_root_element self_closing_tag
(
  (comment (comment_text) @doc)
  .
  (text)
  .
  (svg_root_element
    (self_closing_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; without whitespace — svg_root_element start_tag
(
  (comment (comment_text) @doc)
  .
  (svg_root_element
    (start_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; without whitespace — svg_root_element self_closing_tag
(
  (comment (comment_text) @doc)
  .
  (svg_root_element
    (self_closing_tag
      (attribute
        (id_attribute
          value: (id_attribute_value
            (id_token) @name))))) @definition.id
  (#select-adjacent! @doc @definition.id)
)

; ─── ID definitions (fallback, no docstring) ─────────────────────
; Matches id-bearing elements without an adjacent comment.

(id_attribute
  value: (id_attribute_value
    (id_token) @name)) @definition.id

; ─── ID references (href) ───────────────────────────────────────
; <use href="#foo"/>, <textPath href="#path1">, <a href="#section">

((href_attribute
  value: (href_attribute_value
    (href_reference
      (iri_reference) @name))) @reference.id
 (#match? @name "^#"))

; ─── ID references (paint url()) ────────────────────────────────
; fill="url(#grad1)", stroke="url(#pattern)"

((paint_attribute
  value: (paint_attribute_value
    (paint_value
      (paint_server
        (iri_reference) @name)))) @reference.id
 (#match? @name "^#"))

; ─── ID references (functional IRI) ─────────────────────────────
; clip-path="url(#clip)", mask="url(#mask)", filter="url(#blur)"

((functional_iri_attribute
  value: (functional_iri_attribute_value
    (iri_reference) @name)) @reference.id
 (#match? @name "^#"))

((functional_iri_attribute
  value: (functional_iri_attribute_value
    (functional_iri
      (iri_reference) @name))) @reference.id
 (#match? @name "^#"))
