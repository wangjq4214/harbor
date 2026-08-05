#!/usr/bin/env python3
"""Print protocol checklist coverage from Markdown item markers."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKLIST = ROOT / "docs" / "protocol" / "checklist.md"
SECTION = re.compile(r"(?m)^## (\d+)\. (.+)$")
CHECKED = re.compile(r"(?m)^\* \[x\] ")
OPEN = re.compile(r"(?m)^\* \[ \] ")


def count_items(text: str) -> tuple[int, int]:
    return len(CHECKED.findall(text)), len(OPEN.findall(text))


def main() -> None:
    text = CHECKLIST.read_text(encoding="utf-8")
    matches = list(SECTION.finditer(text))

    print(f"{'Section':<46} {'Done':>6} {'Open':>6} {'Total':>6}")
    print("-" * 68)

    total_done = 0
    total_open = 0
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        done, open_items = count_items(text[match.end() : end])
        total_done += done
        total_open += open_items
        label = f"{match.group(1)}. {match.group(2)}"
        print(f"{label:<46} {done:>6} {open_items:>6} {done + open_items:>6}")

    print("-" * 68)
    print(f"{'Total':<46} {total_done:>6} {total_open:>6} {total_done + total_open:>6}")


if __name__ == "__main__":
    main()
