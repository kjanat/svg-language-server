(comment) @annotation

; ─── Elements with id ───────────────────────────────────────────
(element
  (start_tag
    name: (name) @name
    (attribute
      (id_attribute
        value: (id_attribute_value
          (id_token) @context))))) @item

(element
  (self_closing_tag
    name: (name) @name
    (attribute
      (id_attribute
        value: (id_attribute_value
          (id_token) @context))))) @item

; ─── Elements with class ────────────────────────────────────────
(element
  (start_tag
    name: (name) @name
    (attribute
      (class_attribute
        value: (class_attribute_value
          (class_list
            (class_name) @context)))))) @item

(element
  (self_closing_tag
    name: (name) @name
    (attribute
      (class_attribute
        value: (class_attribute_value
          (class_list
            (class_name) @context)))))) @item

; ─── Referencing elements with href ─────────────────────────────
(element
  (start_tag
    name: (name) @name
    (attribute
      (href_attribute
        value: (href_attribute_value
          (href_reference
            (iri_reference) @context)))))) @item

(element
  (self_closing_tag
    name: (name) @name
    (attribute
      (href_attribute
        value: (href_attribute_value
          (href_reference
            (iri_reference) @context)))))) @item

; ─── Fallback: tag name only ────────────────────────────────────
(svg_root_element
  (start_tag
    name: (name) @name)) @item

(svg_root_element
  (self_closing_tag
    name: (name) @name)) @item

(element
  (start_tag
    name: (name) @name)) @item

(element
  (self_closing_tag
    name: (name) @name)) @item
