; Elements
(svg_root_element) @entry.around
(element) @entry.around
(self_closing_tag) @entry.around

; Attributes
(attribute) @parameter.around
(attribute
  (_ value: (_) @parameter.inside))

; Comments
(comment) @comment.around
(comment text: (comment_text) @comment.inside)

; Functions (transform functions, color functions)
(transform_function) @function.around
(functional_color) @function.around

; Path segments
(path_segment) @entry.around
(moveto_segment) @entry.around
