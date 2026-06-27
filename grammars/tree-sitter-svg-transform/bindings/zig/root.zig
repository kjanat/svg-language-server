extern fn tree_sitter_svg_transform() callconv(.c) *const anyopaque;

pub fn language() *const anyopaque {
    return tree_sitter_svg_transform();
}
