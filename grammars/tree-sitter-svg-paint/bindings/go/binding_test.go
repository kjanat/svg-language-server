package tree_sitter_svg_paint_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_svg_paint "github.com/kjanat/svg/grammars/tree-sitter-svg-paint/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_svg_paint.Language())
	if language == nil {
		t.Errorf("Error loading SVG Paint and Color grammar")
	}
}
