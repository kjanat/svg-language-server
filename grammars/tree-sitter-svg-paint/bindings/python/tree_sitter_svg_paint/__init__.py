"""SVG paint and color grammar for Tree-sitter"""

from importlib.resources import files as _files
from typing import Final

from ._binding import language


def _get_query(file: str) -> str | None:
    try:
        query = _files(f"{__package__}") / file
        return query.read_text()
    except FileNotFoundError:
        return None


HIGHLIGHTS_QUERY: Final[str | None] = _get_query("queries/highlights.scm")
INJECTIONS_QUERY: Final[str | None] = _get_query("queries/injections.scm")
LOCALS_QUERY: Final[str | None] = _get_query("queries/locals.scm")
TAGS_QUERY: Final[str | None] = _get_query("queries/tags.scm")


__all__: Final = [
    "HIGHLIGHTS_QUERY",
    "INJECTIONS_QUERY",
    "LOCALS_QUERY",
    "TAGS_QUERY",
    "language",
]


def __dir__() -> list[str]:
    return sorted(
        __all__
        + [
            "__all__",
            "__builtins__",
            "__cached__",
            "__doc__",
            "__file__",
            "__loader__",
            "__name__",
            "__package__",
            "__path__",
            "__spec__",
        ]
    )
