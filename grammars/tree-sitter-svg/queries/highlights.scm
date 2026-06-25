(xml_declaration) @keyword
(doctype) @keyword

(xml_version_attribute_name) @attribute
(xml_encoding_attribute_name) @attribute
(xml_standalone_attribute_name) @attribute
(xml_standalone_attribute_value) @boolean

(comment) @comment
(cdata_section) @markup.raw
(entity_reference) @string.escape

(processing_instruction
  (name) @keyword)

(start_tag
  (name) @tag)

(end_tag
  (name) @tag)

(self_closing_tag
  (name) @tag)

(erroneous_end_tag
  (name) @tag.error)

; `attribute` is a supertype. Match fields structurally so generated typed
; attributes keep highlighting when catalog buckets change.
(attribute
  name: (_) @attribute)

(animate_motion_coordinate_attribute
  name: (_) @attribute)

(animate_motion_values_attribute
  name: (_) @attribute)

(clip_rect "rect" @function.call)


(attribute
  value: (_) @string)

(animate_motion_coordinate_attribute
  value: (_) @string)

(animate_motion_values_attribute
  value: (_) @string)

; Transform function highlights live in the injected svg_transform grammar.

(style_text_double) @string
(style_text_single) @string
(script_text_double) @string
(script_text_single) @string

(class_name) @link_uri
(iri_reference) @link_uri

; Paint/color highlights live in the injected svg_paint grammar.

(align_keyword) @constant
(meet_or_slice_keyword) @constant

(repeat_count_attribute_value "indefinite" @constant.builtin)
(enable_background_new "new" @keyword)
(enable_background_attribute_value "accumulate" @constant.builtin)
(stroke_dasharray_attribute_value ["none" "inherit"] @constant.builtin)
(duration_attribute_value ["indefinite" "media"] @constant.builtin)

(number) @number

(length_unit) @type
(time_unit) @type
(percentage "%" @type)

; Path data highlights live in the injected svg_path grammar
; (grammars/tree-sitter-svg-path/queries/highlights.scm).

["(" ")"] @punctuation.bracket
["\"" "'"] @punctuation.delimiter

[
  "<?"
  "?>"
  "<"
  ">"
  "</"
  "/>"
  "="
  "<!--"
  "-->"
  "<!DOCTYPE"
  "<![CDATA["
  "]]>"
] @punctuation.delimiter
