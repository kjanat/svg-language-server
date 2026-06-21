(comment) @annotation

; Elements with id get the id as their outline label
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

; Elements without id still appear with tag name only
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
