; Transform-list highlights for the injected svg_transform grammar.
; Relocated from tree-sitter-svg when transform functions were evicted; node
; names are identical, so editor themes keep the same captures.

(matrix_transform "matrix" @function.builtin)
(translate_transform "translate" @function.builtin)
(scale_transform "scale" @function.builtin)
(rotate_transform "rotate" @function.builtin)
(skew_x_transform "skewX" @function.builtin)
(skew_y_transform "skewY" @function.builtin)

(number) @number

["(" ")"] @punctuation.bracket
"," @punctuation.delimiter
