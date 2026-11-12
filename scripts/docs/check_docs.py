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
import tempfile
import unicodedata
from contextlib import contextmanager
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


def git_files(root: Path, *, staged: bool = False) -> set[Path] | None:
    """Enumerate Git candidates; None means a source export without Git metadata."""
    root = root.resolve()
    if not any((parent / ".git").exists() for parent in (root, *root.parents)):
        return None
    options = ["--cached"] if staged else ["--cached", "--others", "--exclude-standard"]
    try:
        result = subprocess.run(
            ["git", "ls-files", *options, "-z", "--", "."],
            cwd=root, capture_output=True, check=True, timeout=30,
        )
        names = {os.fsdecode(name) for name in result.stdout.split(b"\0") if name}
        if staged:
            # Compare two index-only views: worktree edits/deletions and rename
            # detection must not make an intent-to-add entry look staged.
            additions = []
            for visibility in ("visible", "invisible"):
                diff = subprocess.run(
                    ["git", "--no-optional-locks", "diff", "--cached", "--name-only",
                     "--diff-filter=A", "--no-renames", f"--ita-{visibility}-in-index",
                     "-z", "--", "."],
                    cwd=root, capture_output=True, check=True, timeout=30,
                )
                additions.append({os.fsdecode(name) for name in diff.stdout.split(b"\0") if name})
            names.difference_update(additions[0] - additions[1])
    except (OSError, subprocess.SubprocessError) as error:
        raise RuntimeError(f"Git Markdown enumeration failed: {error}") from error
    files = set()
    for name in names:
        path = root / name
        if not staged:
            try:
                path.lstat()
            except FileNotFoundError:
                # Working-tree deletions are not available delivery candidates.
                continue
            if path.is_dir() and not path.is_symlink():
                try:
                    entry = subprocess.run(
                        ["git", "ls-files", "--stage", "-z", "--", name],
                        cwd=root, capture_output=True, check=True, timeout=30,
                    )
                except (OSError, subprocess.SubprocessError) as error:
                    raise RuntimeError(f"Git Markdown enumeration failed: {error}") from error
                if not entry.stdout.startswith(b"160000 "):
                    # A directory replacing an indexed ordinary file is not that file.
                    continue
        files.add(path)
    return files


@contextmanager
def staged_snapshot(root: Path):
    """Materialize only indexed files in a temporary tree; never stage or reset."""
    root = root.resolve()
    files = git_files(root, staged=True)
    if files is None:
        raise RuntimeError("--staged requires Git metadata; source exports have no index.")
    with tempfile.TemporaryDirectory(prefix="harbor-docs-") as directory:
        snapshot = Path(directory).resolve()
        names = {path.relative_to(root).as_posix() for path in files}
        try:
            index = subprocess.run(
                ["git", "ls-files", "--stage", "-z", "--", "."],
                cwd=root, capture_output=True, check=True, timeout=30,
            )
            entries = {}
            for record in index.stdout.split(b"\0"):
                if not record:
                    continue
                metadata, raw_name = record.split(b"\t", 1)
                mode, object_id, stage = metadata.split()
                name = os.fsdecode(raw_name)
                if name in names:
                    if stage != b"0":
                        raise RuntimeError(f"Unmerged staged path: {name}")
                    entries[name] = (mode, object_id)
            if entries.keys() != names:
                raise RuntimeError("Git index changed while preparing the staged snapshot; retry.")
            blobs = [(name, mode, object_id) for name, (mode, object_id) in sorted(entries.items())
                     if mode != b"160000"]
            result = subprocess.run(
                ["git", "cat-file", "--batch"],
                input=b"".join(object_id + b"\n" for _, _, object_id in blobs),
                cwd=root, capture_output=True, check=True, timeout=30,
            )
        except (OSError, subprocess.SubprocessError, ValueError) as error:
            raise RuntimeError(f"Git staged snapshot export failed: {error}") from error
        offset = 0
        symlinks = []
        for name, mode, object_id in blobs:
            end = result.stdout.find(b"\n", offset)
            header = result.stdout[offset:end].split()
            if end < 0 or len(header) != 3 or header[:2] != [object_id, b"blob"] or not header[2].isdigit():
                raise RuntimeError(f"Invalid Git blob response for staged path: {name}")
            size = int(header[2])
            start, offset = end + 1, end + size + 2
            if result.stdout[offset - 1:offset] != b"\n":
                raise RuntimeError(f"Incomplete Git blob response for staged path: {name}")
            content = result.stdout[start:offset - 1]
            destination = snapshot / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            if mode == b"120000":
                symlinks.append((destination, os.fsdecode(content)))
            else:
                destination.write_bytes(content)
        if offset != len(result.stdout):
            raise RuntimeError("Unexpected trailing data in Git staged snapshot export.")
        for name, (mode, _) in entries.items():
            if mode == b"160000":
                # A gitlink delivers the submodule entry, not an untracked local checkout.
                (snapshot / name).mkdir(parents=True, exist_ok=True)
        # Create links only after writing files, so no write follows a staged symlink.
        for destination, target in symlinks:
            destination.symlink_to(Path(target), target_is_directory=(destination.parent / target).is_dir())
        for destination, _ in symlinks:
            if not destination.resolve().is_relative_to(snapshot):
                raise RuntimeError(f"Staged symbolic link escapes snapshot: {destination.relative_to(snapshot)}")
        yield snapshot, {snapshot / name for name in names}


def markdown_files(root: Path = ROOT) -> list[Path]:
    """Select tracked + nonignored untracked Markdown, including crate docs.

    Tracked files are never filtered by ignore rules (even evidence/build paths).
    Removed working-tree sources are excluded; links to them still fail.
    Git failures in a checkout are fatal, not an excuse to scan an empty set.
    """
    root = root.resolve()
    files = git_files(root)
    if files is not None:
        return sorted(path for path in files if path.suffix.lower() == ".md")

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
    # Keep the lexical URL path: resolving now could hide an ignored symlink alias.
    path = Path(os.path.abspath(source.parent / unquote(url.path))) if url.path else source.absolute()
    return path, unquote(url.fragment)


def deliverable_target(
    target: Path, root: Path, files: set[Path], available: set[Path],
) -> bool:
    """Require every traversed symlink and its final target to be deliverable."""
    seen = set()
    while target.is_relative_to(root) and target not in seen:
        seen.add(target)
        parts = target.relative_to(root).parts
        current = root
        for index, part in enumerate(parts):
            current /= part
            if current.is_symlink():
                if current not in files:
                    return False
                target = Path(os.path.abspath(
                    current.parent / current.readlink() / Path(*parts[index + 1:]),
                ))
                break
        else:
            return target in available
    return False


def validate(
    files: list[Path], root: Path, *, available: set[Path] | None = None,
) -> list[str]:
    errors: list[str] = []
    texts: dict[Path, str] = {}
    anchors: dict[Path, set[str]] = {}
    root = root.resolve()
    if available is None:
        available = git_files(root)
    candidates = available
    if available is not None:
        # Git records files, not directories. A directory is deliverable only
        # when it contains a candidate file, not just ignored local evidence.
        available = available.copy()
        for path in tuple(available):
            available.update(parent for parent in path.parents if parent.is_relative_to(root))

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
                elif candidates is not None and not deliverable_target(target, root, candidates, available):
                    errors.append(f"{location}: local link target is not in the Git file set: {raw_target}")
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
            "In a checkout, local targets must belong to the Git file set (directories need a candidate file). "
            "Working-tree mode includes nonignored new files but cannot prove they will be committed; "
            "--staged exports indexed content only and excludes intent-to-add entries. "
            "Checks Han-character policy (including code), inline Markdown links/images, URL-decoded "
            "local paths and Markdown heading/quoted HTML-anchor fragments. Fenced code, inline "
            "code and HTML comments are ignored for links/anchors. Not a full CommonMark parser: "
            "reference/HTML links, nested block/list headings or fences, complex inline markup, "
            "custom renderer IDs and non-Markdown fragments need manual review. No network, "
            "rendering or factual-accuracy checks."
        ),
    )
    parser.add_argument(
        "--staged", action="store_true",
        help="Check indexed content and link targets in an isolated snapshot, not working-tree edits.",
    )
    args = parser.parse_args(argv)
    try:
        if args.staged:
            with staged_snapshot(ROOT) as (root, available):
                files = sorted(path for path in available if path.suffix.lower() == ".md")
                errors = validate(files, root, available=available)
        else:
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

    scope = "staged snapshot" if args.staged else "working tree"
    print(f"Documentation language, local-link and Markdown-fragment checks passed ({len(files)} files). Scope: {scope}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
