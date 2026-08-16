"""Default entry point: launch the interactive terminal UI.

Used by ``python -m copernicus_explorer_py`` and the ``copernicus-explorer``
console script.
"""

from __future__ import annotations

from copernicus_explorer_py import run_tui


def main() -> None:
    run_tui()


if __name__ == "__main__":
    main()
