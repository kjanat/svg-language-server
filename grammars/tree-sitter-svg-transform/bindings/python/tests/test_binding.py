from unittest import TestCase

import tree_sitter_svg_transform

from tree_sitter import Language, Parser


class TestLanguage(TestCase):
    def test_can_load_grammar(self) -> None:
        _ = Parser(Language(tree_sitter_svg_transform.language()))
