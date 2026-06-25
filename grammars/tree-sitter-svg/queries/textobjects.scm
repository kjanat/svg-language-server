; Elements
(svg_root_element) @entry.around
(element) @entry.around
(self_closing_tag) @entry.around

; Attributes
(attribute) @parameter.around
(attribute value: (_) @parameter.inside)
(animate_motion_coordinate_attribute) @parameter.around
(animate_motion_coordinate_attribute value: (_) @parameter.inside)
(animate_motion_values_attribute) @parameter.around
(animate_motion_values_attribute value: (_) @parameter.inside)

; Comments
(comment) @comment.around
(comment text: (comment_text) @comment.inside)

; Functions (color functions; transform fns moved to svg_transform)

; Path segments live in the injected svg_path grammar
; (grammars/tree-sitter-svg-path/queries/textobjects.scm).
