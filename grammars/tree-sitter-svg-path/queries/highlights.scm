; Path-data highlights for the injected svg_path grammar.
; Relocated verbatim from tree-sitter-svg when path data was evicted to its own
; grammar; node names are identical, so editor themes keep the same captures.

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
