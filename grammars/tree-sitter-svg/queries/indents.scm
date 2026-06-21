; Tag-level @indent captures intentionally duplicate the element-level captures
; below (lines 18-24). Editors like Zed query tag-level positions directly for
; indentation rather than walking to the parent element node.
;
; Multi-line tags also align continuation lines to the tag name, which makes
; wrapped attribute lists read more naturally than a plain extra indent.
((start_tag
  name: (name) @anchor) @align
 (#set! "scope" "all"))

((self_closing_tag
  name: (name) @anchor) @align
 (#set! "scope" "all"))

(start_tag ">" @end) @indent
(self_closing_tag "/>" @end) @indent

(element
  (start_tag) @start
  [(end_tag) (erroneous_end_tag)]? @end) @indent

(svg_root_element
  (start_tag) @start
  [(end_tag) (erroneous_end_tag)]? @end) @indent

([(end_tag) (erroneous_end_tag)] @outdent
 (#set! "scope" "all"))
