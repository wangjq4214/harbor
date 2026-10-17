#!/usr/bin/env python3
"""Validate repository Markdown language, local file links and Markdown fragments.

This is a bounded standard-library checker, not a complete Markdown renderer.
"""

from __future__ import annotations

import argparse
import html
import os
import re
import subprocess
import sys
import unicodedata
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
HAN = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff]")
# Inline links/images: angle destinations, one level of balanced parentheses,
# and optional quoted titles. Reference links and HTML href/src are not parsed.
LINK = re.compile(
    r"!?\[[^\]\n]*\]\(\s*(<[^>\n]*>|(?:\\.|[^\\\s()]|\([^()\n]*\))+)"
    r"(?:\s+(?:\"[^\"\n]*\"|'[^'\n]*'|\([^()\n]*\)))?\s*\)"
)
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
ATX = re.compile(r"^ {0,3}#{1,6}(?:[ \t]+(.*?)|[ \t]*)$")
SETEXT = re.compile(r"^ {0,3}(?:=+|-+)[ \t]*$")
INLINE_CODE = re.compile(r"(`+)(?!`)(.+?)(?<!`)\1(?!`)", re.DOTALL)
HTML_ANCHOR = re.compile(
    r"<[a-zA-Z][^>]*?\s(?:id|name)\s*=\s*([\"'])(.*?)\1", re.IGNORECASE
)
# An exported tree has no Git ignore information. Only conventional source
# roots are scanned there; local evidence under docs is deliberately included.
SOURCE_ROOTS = ("docs", ".grimoire", "scripts", "crates", "third_party")
GENERATED_DIRS = {".git", "target", "node_modules", "__pycache__", ".venv"}


def markdown_files(root: Path = ROOT) -> list[Path]:
    """Select tracked + nonignored untracked Markdown, including crate docs.

    Tracked files are never filtered by ignore rules (even evidence/build paths).
    Removed working-tree sources are excluded; links to them still fail.
    Git failures in a checkout are fatal, not an excuse to scan an empty set.
    """
    root = root.resolve()
    if any((parent / ".git").exists() for parent in (root, *root.parents)):
        try:
            result = subprocess.run(
                ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "."],
                cwd=root, capture_output=True, check=True, timeout=30,
            )
        except (OSError, subprocess.SubprocessError) as error:
            raise RuntimeError(f"Git Markdown enumeration failed: {error}") from error
        names = {os.fsdecode(name) for name in result.stdout.split(b"\0") if name}
        files = []
        for name in names:
            if Path(name).suffix.lower() != ".md":
                continue
            path = root / name
            try:
                path.lstat()
            except FileNotFoundError:
                # A worktree deletion needs no staging; surviving incoming links
                # still fail validation. Other stat/read errors remain failures.
                continue
            files.append(path)
        return sorted(files)

    print(
        "No Git metadata: source-tree fallback checks root Markdown and "
        f"{', '.join(SOURCE_ROOTS)} recursively; Git ignores are unavailable "
        "(local docs/evidence included; generated directories excluded).",
        file=sys.stderr,
    )
    files = {path for path in root.iterdir() if path.is_file() and path.suffix.lower() == ".md"}
    for name in SOURCE_ROOTS:
        source = root / name
        if not source.is_dir():
            continue
        # os.walk does not follow directory symlinks; report traversal failures.
        def fail(error: OSError) -> None:
            raise error
        for directory, dirs, names in os.walk(source, onerror=fail):
            dirs[:] = sorted(d for d in dirs if d not in GENERATED_DIRS)
            files.update(Path(directory) / name for name in names if Path(name).suffix.lower() == ".md")
    return sorted(files)


def prose(text: str) -> str:
    """Mask fenced blocks and HTML comments, retaining source line numbers."""
    lines = []
    fence_char, fence_length = "", 0
    for line in text.splitlines():
        if fence_char:
            if re.fullmatch(r" {0,3}" + re.escape(fence_char) + "{" + str(fence_length) + r",}[ \t]*", line):
                fence_char = ""
            lines.append("")
            continue
        match = FENCE.match(line)
        if match:
            fence_char, fence_length = match[1][0], len(match[1])
            lines.append("")
        else:
            lines.append(line)
    return re.sub(r"<!--[\s\S]*?-->", lambda m: "\n" * m[0].count("\n"), "\n".join(lines))


def heading_ids(text: str) -> set[str]:
    """Common GitHub-style ATX/Setext IDs, duplicates, and quoted HTML anchors."""
    visible = prose(text)
    anchors = {html.unescape(m[2]) for m in HTML_ANCHOR.finditer(INLINE_CODE.sub("", visible))}
    generated: set[str] = set()
    previous = ""
    for line in visible.splitlines():
        match = ATX.match(line)
        heading = None
        if match:
            heading = re.sub(r"[ \t]+#+[ \t]*$", "", match[1] or "")
        elif previous.strip() and SETEXT.match(line):
            heading = previous.strip()
        if heading is not None:
            heading = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)
            heading = html.unescape(re.sub(r"<[^>]+>", "", heading))
            # Inline formatting delimiters do not appear in rendered headings.
            heading = re.sub(r"(?<!\w)(_+)(.+?)\1(?!\w)", r"\2", heading)
            heading = heading.replace("`", "").replace("*", "").replace("~", "")
            slug = "".join(
                char for char in heading.lower()
                if char in "-_" or char.isspace() or unicodedata.category(char)[0] in "LNM"
            )
            slug = re.sub(r"\s", "-", slug.strip())
            candidate, suffix = slug, 0
            while candidate in generated:
                suffix += 1
                candidate = f"{slug}-{suffix}"
            generated.add(candidate)
        previous = "" if heading is not None else line
    return anchors | generated


def local_target(source: Path, raw: str) -> tuple[Path, str] | None:
    """Resolve a local URL; decode path/fragment separately, once, after splitting."""
    target = html.unescape(raw.strip())
    if target.startswith("<") and target.endswith(">"):
        target = target[1:-1]
    target = re.sub(r"\\([\\`*{}\[\]()#+\-.!_>])", r"\1", target)
    url = urlsplit(target)
    if not target or url.scheme or url.netloc or target.startswith("//"):
        return None
    path = (source.parent / unquote(url.path)).resolve() if url.path else source.resolve()
    return path, unquote(url.fragment)


def validate(files: list[Path], root: Path) -> list[str]:
    errors: list[str] = []
    texts: dict[Path, str] = {}
    anchors: dict[Path, set[str]] = {}

    def read(path: Path) -> str:
        if path not in texts:
            texts[path] = path.read_text(encoding="utf-8")
        return texts[path]

    for path in files:
        relative = path.relative_to(root).as_posix()
        try:
            text = read(path)
        except (OSError, UnicodeError) as error:
            errors.append(f"{relative}: cannot read Markdown: {error}")
            continue
        for line_number, line in enumerate(text.splitlines(), 1):
            if HAN.search(line):
                errors.append(f"{relative}:{line_number}: non-English Han character")

        visible = INLINE_CODE.sub(lambda m: "\n" * m[0].count("\n"), prose(text))
        for match in LINK.finditer(visible):
            raw_target = match[1]
            location = f"{relative}:{visible.count(chr(10), 0, match.start()) + 1}"
            try:
                resolved = local_target(path, raw_target)
                if resolved is None:
                    continue
                target, fragment = resolved
                if not target.exists():
                    errors.append(f"{location}: broken local link: {raw_target}")
                elif fragment and target.suffix.lower() == ".md" and target.is_file():
                    if target not in anchors:
                        anchors[target] = heading_ids(read(target))
                    if fragment not in anchors[target]:
                        errors.append(f"{location}: missing Markdown fragment: {raw_target}")
            except (OSError, UnicodeError, ValueError) as error:
                errors.append(f"{location}: cannot check local link {raw_target}: {error}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog=(
            "Scope: all tracked and nonignored untracked .md files, including crates and .grimoire. "
            "Reads existing working-tree sources, not the staged snapshot; incoming links to deleted files still fail. Without .git metadata, scans "
            "conventional source roots with an explicit warning; Git failures in checkouts fail. "
            "Checks Han-character policy (including code), inline Markdown links/images, URL-decoded "
            "local paths and Markdown heading/quoted HTML-anchor fragments. Fenced code, inline "
            "code and HTML comments are ignored for links/anchors. Not a full CommonMark parser: "
            "reference/HTML links, nested block/list headings or fences, complex inline markup, "
            "custom renderer IDs and non-Markdown fragments need manual review. No network, "
            "rendering or factual-accuracy checks."
        ),
    )
    parser.parse_args(argv)
    try:
        files = markdown_files(ROOT)
        errors = validate(files, ROOT)
        if not files:
            errors.append("No Markdown files selected; check the source tree.")
    except (OSError, RuntimeError) as error:
        errors = [str(error)]
        files = []
    if errors:
        print("Documentation validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"Documentation language, local-link and Markdown-fragment checks passed ({len(files)} files).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
