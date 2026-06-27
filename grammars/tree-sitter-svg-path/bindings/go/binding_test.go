package tree_sitter_svg_path_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_svg_path "github.com/kjanat/svg/grammars/tree-sitter-svg-path/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_svg_path.Language())
	if language == nil {
		t.Errorf("Error loading SVG Path Data grammar")
	}
}
