; CSS in <style> element (via CDATA)
; CSS in <style> element (via raw_text)
((element
  (start_tag (name) @_start)
  (cdata_section (cdata_text) @injection.content)
  (end_tag (name) @_end))
 (#match? @_start "(^|:)style$")
 (#match? @_end "(^|:)style$")
 (#set! injection.language "css"))

((element
  (start_tag (name) @_start)
  (raw_text)
  (cdata_section (cdata_text) @injection.content)
  (end_tag (name) @_end))
 (#match? @_start "(^|:)style$")
 (#match? @_end "(^|:)style$")
 (#set! injection.language "css"))

((element
  (start_tag (name) @_start)
  (raw_text) @injection.content
  (end_tag (name) @_end))
 (#match? @_start "(^|:)style$")
 (#match? @_end "(^|:)style$")
 (#set! injection.language "css"))

; JS in <script> element (via CDATA)
; JS in <script> element (via raw_text)
((element
  (start_tag (name) @_start)
  (cdata_section (cdata_text) @injection.content)
  (end_tag (name) @_end))
 (#match? @_start "(^|:)script$")
 (#match? @_end "(^|:)script$")
 (#set! injection.language "javascript"))

((element
  (start_tag (name) @_start)
  (raw_text)
  (cdata_section (cdata_text) @injection.content)
  (end_tag (name) @_end))
 (#match? @_start "(^|:)script$")
 (#match? @_end "(^|:)script$")
 (#set! injection.language "javascript"))

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

; SVG path data in d="..." attribute (rich CST via injected svg_path grammar)
((d_attribute
  (d_attribute_value
    (double_quoted_path_data
      (path_data_payload) @injection.content)))
 (#set! injection.language "svg_path"))

((d_attribute
  (d_attribute_value
    (single_quoted_path_data
      (path_data_payload) @injection.content)))
 (#set! injection.language "svg_path"))

; SVG transform list in transform/gradientTransform/patternTransform attribute
((transform_attribute
  (transform_attribute_value
    (transform_payload) @injection.content))
 (#set! injection.language "svg_transform"))

; SVG paint/color in fill/stroke/stop-color/... attribute
((paint_attribute
  (paint_attribute_value
    (paint_payload) @injection.content))
 (#set! injection.language "svg_paint"))

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
