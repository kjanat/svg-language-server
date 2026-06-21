from unittest import TestCase

import tree_sitter_svg
from tree_sitter import Language, Parser


class TestLanguage(TestCase):
    def test_can_load_grammar(self):
        try:
            Parser(Language(tree_sitter_svg.language()))
        except Exception:
            self.fail("Error loading SVG grammar")
