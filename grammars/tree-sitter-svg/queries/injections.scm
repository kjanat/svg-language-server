; CSS in <style> element (via raw_text)
((element
  (start_tag (name) @_start)
  (raw_text) @injection.content
  (end_tag (name) @_end))
 (#match? @_start "(^|:)style$")
 (#match? @_end "(^|:)style$")
 (#set! injection.language "css"))

; JS in <script> element (via raw_text)
((element
  (start_tag (name) @_start)
  (raw_text) @injection.content
  (end_tag (name) @_end))
 (#match? @_start "(^|:)script$")
 (#match? @_end "(^|:)script$")
 (#set! injection.language "javascript"))

; HTML in <foreignObject> — element children
((element
  (start_tag (name) @_start)
  (element) @injection.content
  (end_tag (name) @_end))
 (#match? @_start "(^|:)foreignObject$")
 (#match? @_end "(^|:)foreignObject$")
 (#set! injection.language "html")
 (#set! injection.combined)
 (#set! injection.include-children))

; HTML in <foreignObject> — text children
((element
  (start_tag (name) @_start)
  (text) @injection.content
  (end_tag (name) @_end))
 (#match? @_start "(^|:)foreignObject$")
 (#match? @_end "(^|:)foreignObject$")
 (#set! injection.language "html")
 (#set! injection.combined))

; CSS in style="..." attribute
((style_attribute
  (style_attribute_value
    (double_quoted_style_value
      (style_text_double) @injection.content)))
 (#set! injection.language "css"))

((style_attribute
  (style_attribute_value
    (single_quoted_style_value
      (style_text_single) @injection.content)))
 (#set! injection.language "css"))

; JS in event attributes (onclick, onload, etc.)
((event_attribute
  (event_attribute_value
    (script_text_double) @injection.content))
 (#set! injection.language "javascript"))

((event_attribute
  (event_attribute_value
    (script_text_single) @injection.content))
 (#set! injection.language "javascript"))

; CSS in generic style="..." (fallback for when style parsed as generic_attribute)
((generic_attribute
  name: (attribute_name) @_name
  value: (quoted_attribute_value
    (attribute_text_double) @injection.content))
 (#match? @_name "(^|:)style$")
 (#set! injection.language "css"))

((generic_attribute
  name: (attribute_name) @_name
  value: (quoted_attribute_value
    (attribute_text_single) @injection.content))
 (#match? @_name "(^|:)style$")
 (#set! injection.language "css"))
