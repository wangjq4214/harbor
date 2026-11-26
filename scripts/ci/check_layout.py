#!/usr/bin/env python3
"""Validate Harbor's standardized repository file and module layout."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys
from typing import Iterable

ROOT = Path(__file__).resolve().parents[2]
ALLOWED_TOP_LEVEL_DIRS = frozenset(
    {
        ".github",
        ".grimoire",
        "assets",
        "crates",
        "docs",
        "fuzz",
        "installer",
        "scripts",
        "src",
        "tests",
        "third_party",
    }
)
GENERATED_DIRS = {".git", "target", "node_modules", "__pycache__", ".venv"}


def git_files(root: Path, *, staged: bool = False) -> list[str] | None:
    """Enumerate Git candidates as POSIX relative paths; None if no Git metadata."""
    root = root.resolve()
    if not any((parent / ".git").exists() for parent in (root, *root.parents)):
        return None
    options = ["--cached"] if staged else ["--cached", "--others", "--exclude-standard"]
    try:
        result = subprocess.run(
            ["git", "ls-files", *options, "-z", "--", "."],
            cwd=root,
            capture_output=True,
            check=True,
            timeout=30,
        )
        names = {os.fsdecode(name) for name in result.stdout.split(b"\0") if name}
        if staged:
            additions = []
            for visibility in ("visible", "invisible"):
                diff = subprocess.run(
                    [
                        "git",
                        "--no-optional-locks",
                        "diff",
                        "--cached",
                        "--name-only",
                        "--diff-filter=A",
                        "--no-renames",
                        f"--ita-{visibility}-in-index",
                        "-z",
                        "--",
                        ".",
                    ],
                    cwd=root,
                    capture_output=True,
                    check=True,
                    timeout=30,
                )
                additions.append(
                    {os.fsdecode(name) for name in diff.stdout.split(b"\0") if name}
                )
            names.difference_update(additions[0] - additions[1])
    except (OSError, subprocess.SubprocessError) as error:
        raise RuntimeError(f"Git file enumeration failed: {error}") from error

    paths: list[str] = []
    for name in sorted(names):
        path = root / name
        if not staged:
            try:
                path.lstat()
            except FileNotFoundError:
                continue
            if path.is_dir() and not path.is_symlink():
                try:
                    entry = subprocess.run(
                        ["git", "ls-files", "--stage", "-z", "--", name],
                        cwd=root,
                        capture_output=True,
                        check=True,
                        timeout=30,
                    )
                except (OSError, subprocess.SubprocessError) as error:
                    raise RuntimeError(f"Git file enumeration failed: {error}") from error
                if not entry.stdout.startswith(b"160000 "):
                    continue
        paths.append(name.replace("\\", "/"))
    return paths


def walk_source_tree(root: Path) -> list[str]:
    """Fallback enumeration when running on an exported tree without Git metadata."""
    root = root.resolve()
    paths: list[str] = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(name for name in dirnames if name not in GENERATED_DIRS)
        rel_dir = Path(dirpath).relative_to(root)
        for filename in sorted(filenames):
            rel = (rel_dir / filename).as_posix()
            if rel.startswith("./"):
                rel = rel[2:]
            paths.append(rel)
    return paths


def _is_crate_src_root(parts: list[str]) -> bool:
    return (len(parts) == 2 and parts[0] == "src") or (
        len(parts) == 4 and parts[0] == "crates" and parts[2] == "src"
    )


def _is_under_crate_src(parts: list[str]) -> bool:
    return (len(parts) >= 2 and parts[0] == "src") or (
        len(parts) >= 4 and parts[0] == "crates" and parts[2] == "src"
    )


def validate_paths(paths: Iterable[str]) -> list[str]:
    """Check standardized layout invariants across POSIX relative file paths."""
    errors: list[str] = []
    for raw in sorted(set(paths)):
        rel = raw.replace("\\", "/")
        if rel.startswith("./"):
            rel = rel[2:]
        if not rel:
            continue
        parts = rel.split("/")

        # 1. Tracked top-level directories must be on the explicit allowlist.
        if len(parts) > 1 and parts[0] not in ALLOWED_TOP_LEVEL_DIRS:
            errors.append(
                f"[top-level-dir] Disallowed top-level directory '{parts[0]}': {rel}"
            )

        # 2. Zero mod.rs files anywhere under src/ or crates/*/src/.
        if _is_under_crate_src(parts) and parts[-1] == "mod.rs":
            errors.append(
                f"[mod-rs] Prohibited mod.rs file (use <module>.rs + <module>/): {rel}"
            )

        # 3. Zero flat *_tests.rs files directly under src/ or crates/*/src/.
        if _is_crate_src_root(parts) and parts[-1].endswith("_tests.rs"):
            errors.append(
                f"[flat-tests] Flat *_tests.rs at crate src root (move to <module>/tests.rs or tests/): {rel}"
            )

        # 4. docs/ and scripts/ permit only README.md directly at their root.
        if len(parts) == 2 and parts[0] in ("docs", "scripts") and parts[1] != "README.md":
            errors.append(
                f"[root-readme-only] Only README.md is allowed directly under {parts[0]}/: {rel}"
            )

    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--staged",
        action="store_true",
        help="Check Git-indexed paths only rather than working-tree candidates.",
    )
    args = parser.parse_args(argv)

    try:
        candidates = git_files(ROOT, staged=args.staged)
        if candidates is None:
            if args.staged:
                raise RuntimeError("--staged requires Git metadata.")
            candidates = walk_source_tree(ROOT)
        if not candidates:
            errors = ["No repository files selected; check the source tree."]
        else:
            errors = validate_paths(candidates)
    except (OSError, RuntimeError) as error:
        errors = [str(error)]
        candidates = []

    if errors:
        print("Repository layout validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    scope = "staged index" if args.staged else "working tree"
    print(f"Repository layout check passed ({len(candidates)} files). Scope: {scope}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
