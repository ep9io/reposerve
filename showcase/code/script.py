"""Walks a directory and prints the size of each Markdown file.

Small on purpose: the page exists to show Python colouring.
"""

from __future__ import annotations

from pathlib import Path

SKIP = {".git", "target", "node_modules"}


def markdown_files(root: Path) -> list[Path]:
    """Every Markdown file below `root`, in path order."""
    return sorted(
        path
        for path in root.rglob("*.md")
        if not SKIP.intersection(path.parts)
    )


def main() -> None:
    total = 0
    for path in markdown_files(Path(".")):
        size = path.stat().st_size
        total += size
        print(f"{path.name:<24} {size:>8,} bytes")
    print(f"{'total':<24} {total:>8,} bytes")


if __name__ == "__main__":
    main()
