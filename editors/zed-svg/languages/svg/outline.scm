(comment) @annotation

; Elements with id get the id as their outline label
(element
  (start_tag
    name: (name) @name
    (id_attribute
      value: (id_attribute_value
        (id_token) @context)))) @item

(element
  (self_closing_tag
    name: (name) @name
    (id_attribute
      value: (id_attribute_value
        (id_token) @context)))) @item

; Zed-side context: class-bearing elements show the class name.
(element
  (start_tag
    name: (name) @name
    (class_attribute
      value: (class_attribute_value
        (class_list
          (class_name) @context))))) @item

(element
  (self_closing_tag
    name: (name) @name
    (class_attribute
      value: (class_attribute_value
        (class_list
          (class_name) @context))))) @item

; Zed-side context: href-bearing elements show their target.
(element
  (start_tag
    name: (name) @name
    (href_attribute
      value: (href_attribute_value
        (href_reference
          (iri_reference) @context))))) @item

(element
  (self_closing_tag
    name: (name) @name
    (href_attribute
      value: (href_attribute_value
        (href_reference
          (iri_reference) @context))))) @item

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
