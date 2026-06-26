from types import CapsuleType
from typing import Final

HIGHLIGHTS_QUERY: Final[str | None]
"""The syntax highlighting query for this grammar."""

INJECTIONS_QUERY: Final[str | None]
"""The language injection query for this grammar."""

LOCALS_QUERY: Final[str | None]
"""The local variable query for this grammar."""

TAGS_QUERY: Final[str | None]
"""The symbol tagging query for this grammar."""

def language() -> CapsuleType:
    """The tree-sitter language function for this grammar."""
