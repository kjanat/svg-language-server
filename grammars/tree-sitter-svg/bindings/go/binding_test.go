package tree_sitter_svg_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_svg "github.com/kjanat/tree-sitter-svg/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_svg.Language())
	if language == nil {
		t.Errorf("Error loading SVG grammar")
	}
}
