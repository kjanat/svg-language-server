package tree_sitter_svg_transform_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_svg_transform "github.com/kjanat/svg/grammars/tree-sitter-svg-transform/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_svg_transform.Language())
	if language == nil {
		t.Errorf("Error loading SVG Transform List grammar")
	}
}
