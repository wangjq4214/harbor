#!/usr/bin/env python3
"""Validate Harbor Markdown language policy and local links."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
DOC_ROOTS = (ROOT / "README.md", ROOT / "docs", ROOT / ".grimoire")
HAN = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff]")
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
SCHEME = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")


def markdown_files() -> list[Path]:
    files: list[Path] = []
    for root in DOC_ROOTS:
        if root.is_file():
            files.append(root)
        elif root.is_dir():
            files.extend(root.rglob("*.md"))
    return sorted(files)


def local_target(source: Path, raw: str) -> Path | None:
    target = raw.strip()
    if not target or target.startswith("#") or SCHEME.match(target):
        return None
    if target.startswith("<") and target.endswith(">"):
        target = target[1:-1]
    target = unquote(target.split("#", 1)[0])
    if not target:
        return None
    return (source.parent / target).resolve()


def main() -> int:
    errors: list[str] = []
    for path in markdown_files():
        text = path.read_text(encoding="utf-8")
        relative = path.relative_to(ROOT)

        for line_number, line in enumerate(text.splitlines(), 1):
            if HAN.search(line):
                errors.append(f"{relative}:{line_number}: non-English Han character")

        for raw_target in LINK.findall(text):
            target = local_target(path, raw_target)
            if target is not None and not target.exists():
                errors.append(f"{relative}: broken local link: {raw_target}")

    if errors:
        print("Documentation validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("Documentation language and local-link checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
