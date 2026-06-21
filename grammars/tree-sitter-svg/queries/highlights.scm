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

(attribute_name) @attribute
(class_attribute_name) @attribute
(clip_attribute_name) @attribute
(d_attribute_name) @attribute
(duration_attribute_name) @attribute
(enable_background_attribute_name) @attribute
(event_attribute_name) @attribute
(functional_iri_attribute_name) @attribute
(href_attribute_name) @attribute
(id_attribute_name) @attribute
(key_splines_attribute_name) @attribute
(key_times_attribute_name) @attribute
(length_attribute_name) @attribute
(length_list_attribute_name) @attribute
(number_attribute_name) @attribute
(number_list_attribute_name) @attribute
(offset_attribute_name) @attribute
(opacity_attribute_name) @attribute
(paint_attribute_name) @attribute
(points_attribute_name) @attribute
(preserve_aspect_ratio_attribute_name) @attribute
(repeat_count_attribute_name) @attribute
(stroke_dasharray_attribute_name) @attribute
(style_attribute_name) @attribute
(transform_attribute_name) @attribute
(viewbox_attribute_name) @attribute

(hex_color) @constant
(color_function_name) @function.call
(clip_rect "rect" @function.call)
(named_color) @constant

(color_colorspace) @type.builtin
(color_interpolation_space) @type.builtin
(color_mix_in_keyword) @keyword
(color_hue_direction) @keyword.modifier
(color_hue_keyword) @keyword
(color_none) @constant.builtin

[(quoted_attribute_value)
 (class_attribute_value)
 (clip_attribute_value)
 (d_attribute_value)
 (duration_attribute_value)
 (enable_background_attribute_value)
 (event_attribute_value)
 (functional_iri_attribute_value)
 (href_attribute_value)
 (id_attribute_value)
 (key_splines_attribute_value)
 (key_times_attribute_value)
 (length_attribute_value)
 (length_list_attribute_value)
 (number_attribute_value)
 (number_list_attribute_value)
 (offset_attribute_value)
 (opacity_attribute_value)
 (paint_attribute_value)
 (points_attribute_value)
 (preserve_aspect_ratio_attribute_value)
 (repeat_count_attribute_value)
 (stroke_dasharray_attribute_value)
 (style_attribute_value)
 (transform_attribute_value)
 (viewbox_attribute_value)] @string

(matrix_transform "matrix" @function.builtin)
(translate_transform "translate" @function.builtin)
(scale_transform "scale" @function.builtin)
(rotate_transform "rotate" @function.builtin)
(skew_x_transform "skewX" @function.builtin)
(skew_y_transform "skewY" @function.builtin)

(style_text_double) @string
(style_text_single) @string
(script_text_double) @string
(script_text_single) @string

(class_name) @link_uri
(iri_reference) @link_uri

(paint_value
  ["none" "currentColor" "context-fill" "context-stroke" "inherit"] @constant.builtin)

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
(angle_unit) @type
(percentage "%" @type)

((path_command) @constructor
  (#match? @constructor "^[Mm]$"))

((path_command) @keyword
  (#match? @keyword "^[LlHhVv]$"))

((path_command) @function.builtin
  (#match? @function.builtin "^[CcSsQqTt]$"))

((path_command) @function
  (#match? @function "^[Aa]$"))

((path_command) @punctuation.special
  (#match? @punctuation.special "^[Zz]$"))

; `@property`/`@constant` are intentional here for visual differentiation of
; coordinate roles in path data. Keep these captures stable for editor themes
; that only recognize the canonical highlight namespace.
(path_coordinate_pair
  (path_coordinate
    (path_number) @number)
  (path_comma_wsp)
  (path_coordinate
    (path_number) @property))

(path_coordinate_pair
  (path_coordinate
    (path_number) @number)
  (path_coordinate
    (path_number) @property))

(horizontal_lineto_segment
  (path_coordinate
    (path_number) @number))

(vertical_lineto_segment
  (path_coordinate
    (path_number) @property))

(elliptical_arc_radii
  (path_coordinate
    (path_number) @constant)
  (path_comma_wsp)
  (path_coordinate
    (path_number) @constant))

(elliptical_arc_radii
  (path_coordinate
    (path_number) @constant)
  (path_coordinate
    (path_number) @constant))

(path_rotation) @number
(path_arc_flag) @boolean
(path_sweep_flag) @boolean
(path_comma) @punctuation.delimiter

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
