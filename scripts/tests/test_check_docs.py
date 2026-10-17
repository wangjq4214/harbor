"""Portable Markdown validation tests; no network or checkout mutations."""
import contextlib
import importlib.util
import io
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1] / "docs" / "check_docs.py"
spec = importlib.util.spec_from_file_location("check_docs", PATH)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class TemporaryTree(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def write(self, name, text=""):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def run_main(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(checker, "ROOT", self.root), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = checker.main([])
        return code, stdout.getvalue(), stderr.getvalue()


@unittest.skipUnless(shutil.which("git"), "Git is required for real index/ignore selection tests")
class GitSelectionTests(TemporaryTree):
    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True)

    def setUp(self):
        super().setUp()
        self.git("init", "-q")

    def test_tracked_new_ignored_and_force_tracked_evidence(self):
        self.write(".gitignore", "/docs/verification/*\n/target/\n/local/\n*.log\n")
        self.write("README.md", "# Root\n")
        self.write("crates/library/README.md", "# Library\n")
        self.write("docs/verification/tracked.md", "[missing](missing.log)\n")
        self.write("target/tracked.md", "# Tracked despite build-directory ignore\n")
        self.git("add", "README.md", "crates/library/README.md")
        self.git("add", "-f", "docs/verification/tracked.md", "target/tracked.md")
        self.write("docs/new page.md", "# New page\n")
        self.write("crates/new-library/guide.MD", "# New library\n")
        self.write("docs/verification/local.md", "[ignored](missing.log)\n")
        self.write("target/local.md", "[ignored](missing.log)\n")
        self.write("local/private.md", "[ignored](missing.log)\n")
        self.write("docs/not-markdown.txt", "ignored\n")
        selected = checker.markdown_files(self.root)
        self.assertEqual(
            {path.relative_to(self.root).as_posix() for path in selected},
            {"README.md", "crates/library/README.md", "docs/verification/tracked.md",
             "target/tracked.md", "docs/new page.md", "crates/new-library/guide.MD"},
        )
        self.assertEqual(len(selected), len(set(selected)))
        errors = checker.validate(selected, self.root)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("docs/verification/tracked.md:1: broken local link", errors[0])
        self.assertTrue((self.root / "docs/verification/local.md").exists())

    def test_new_crate_markdown_fails_main_before_git_add(self):
        self.write("crates/new/README.md", "[missing](not-created.rs#anything)\n")
        code, stdout, stderr = self.run_main()
        self.assertEqual(code, 1)
        self.assertNotIn("passed", stdout)
        self.assertIn("crates/new/README.md:1: broken local link", stderr)

    def test_ignored_evidence_is_not_a_source_but_missing_targets_still_fail(self):
        source = self.write("README.md", "# Public docs\n")
        self.write(".gitignore", "/docs/verification/*\n")
        self.write("docs/verification/local.md", "[local-only](missing.log)\n")
        self.git("add", "README.md")
        code, stdout, stderr = self.run_main()
        self.assertEqual(code, 0, stderr)
        self.assertIn("passed (1 files)", stdout)
        source.write_text("[artifact](docs/verification/missing.log)\n", encoding="utf-8")
        code, stdout, stderr = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("broken local link: docs/verification/missing.log", stderr)

    def test_worktree_not_index_contents_and_deleted_sources_are_excluded(self):
        path = self.write("README.md", "# Indexed\n")
        self.git("add", "README.md")
        path.write_text("# Working tree\n[missing](absent.md)\n", encoding="utf-8")
        self.assertIn("broken local link", checker.validate(checker.markdown_files(self.root), self.root)[0])
        path.unlink()  # Only this test's temporary file, never checkout evidence.
        self.assertEqual(checker.markdown_files(self.root), [])
        self.assertIn("cannot read Markdown", checker.validate([path], self.root)[0])

    def test_unstaged_retirement_passes_only_after_incoming_links_are_repaired(self):
        root = self.write("README.md", "[old](docs/retired.md)\n")
        retired = self.write("docs/retired.md", "# Superseded\n")
        self.git("add", "README.md", "docs/retired.md")
        retired.unlink()
        self.assertEqual(checker.markdown_files(self.root), [root])
        code, _, stderr = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("broken local link: docs/retired.md", stderr)
        root.write_text("# Updated navigation\n", encoding="utf-8")
        code, stdout, stderr = self.run_main()
        self.assertEqual(code, 0, stderr)
        self.assertIn("passed (1 files)", stdout)


class EnumerationFailureTests(TemporaryTree):
    def test_git_errors_never_fall_back_or_pass(self):
        self.write(".git", "gitdir: missing-worktree-metadata\n")
        self.write("README.md", "# Otherwise valid\n")
        for failure in [FileNotFoundError("git unavailable"),
                        subprocess.CalledProcessError(128, ["git", "ls-files"]),
                        subprocess.TimeoutExpired(["git"], 30)]:
            with self.subTest(failure=type(failure).__name__), patch.object(checker.subprocess, "run", side_effect=failure):
                code, stdout, stderr = self.run_main()
                self.assertEqual(code, 1)
                self.assertNotIn("passed", stdout)
                self.assertIn("Git Markdown enumeration failed", stderr)
                self.assertNotIn("fallback", stderr)

    def test_source_export_fallback_is_bounded_and_explicit(self):
        expected = [self.write("README.md"), self.write("docs/new.md"),
                    self.write("crates/lib/README.md"), self.write("scripts/README.md"),
                    self.write(".grimoire/contract.md"), self.write("third_party/runtime/README.md"),
                    self.write("docs/verification/evidence.md")]
        self.write("target/generated.md", "[missing](gone)\n")
        self.write("crates/lib/target/generated.md", "[missing](gone)\n")
        self.write(".pi/local.md", "[missing](gone)\n")
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr), patch.object(checker.subprocess, "run", side_effect=AssertionError("No Git in archive")):
            self.assertEqual(checker.markdown_files(self.root), sorted(expected))
        self.assertIn("source-tree fallback", stderr.getvalue())
        self.assertIn("Git ignores are unavailable", stderr.getvalue())
        self.assertIn("local docs/evidence included", stderr.getvalue())

    def test_empty_export_is_not_a_success(self):
        code, stdout, stderr = self.run_main()
        self.assertEqual(code, 1)
        self.assertNotIn("passed", stdout)
        self.assertIn("No Markdown files selected", stderr)

    def test_unreadable_utf8_is_reported(self):
        path = self.root / "README.md"
        path.write_bytes(b"\xff")
        self.assertIn("cannot read Markdown", checker.validate([path], self.root)[0])


class LinkTests(TemporaryTree):
    def test_local_url_decoding_relative_paths_queries_and_fragments(self):
        source = self.write("docs/source.md")
        cases = [
            ("../crates/lib/README.md#api", self.root / "crates/lib/README.md", "api"),
            ("<nested/a%20b%23c.md?view=1#caf%C3%A9>", self.root / "docs/nested/a b#c.md", "café"),
            ("nested/a%2520b.md#%2520", self.root / "docs/nested/a%20b.md", "%20"),
            ("#same-page", source, "same-page"),
        ]
        for raw, target, fragment in cases:
            with self.subTest(raw=raw):
                self.assertEqual(checker.local_target(source, raw), (target.resolve(), fragment))
        for raw in ["https://example.invalid/a.md#absent", "<https://example.invalid>",
                    "mailto:help@example.invalid", "data:text/plain,anything", "//example.invalid/x", ""]:
            with self.subTest(raw=raw):
                self.assertIsNone(checker.local_target(source, raw))

    def test_existing_files_headings_explicit_anchors_and_inline_destinations(self):
        self.write("docs/a b.md", "# API: `Thing` & *Friends*!\n## Repeat\n## Repeat\n## Repeat-1\n\nSetext Title\n---\n\n<a id=\"Exact.Anchor\"></a>\n<a name='legacy'></a>\n")
        self.write("docs/nested/file(1).md", "# Nested\n")
        self.write("assets/image.png")
        self.write("src/lib.rs", "// no heading IDs here\n")
        source = self.write("docs/source.md", """# Same Page
[space](<a b.md#api-thing--friends> "title")
[encoded](a%20b.md#repeat-1)
[collision](a%20b.md#repeat-1-1)
[setext](a%20b.md#setext-title)
[explicit](a%20b.md#Exact.Anchor)
[legacy](a%20b.md#legacy)
[balanced](nested/file(1).md#nested)
![asset](../assets/image.png 'Image title')
[source](../src/lib.rs#L1-L3)
[folder](nested/)
[same](#same-page)
[external](https://example.invalid/not-checked#missing)
""")
        self.assertEqual(checker.validate([source], self.root), [])

    def test_missing_files_and_missing_same_or_other_page_fragments_fail(self):
        self.write("docs/other.md", "# Exists\n")
        source = self.write("docs/source.md", """# Start
[missing file](gone.md#even-with-fragment)
[missing heading](other.md#absent)
[same missing](#absent)
![missing image](missing.png)
[case-sensitive](other.md#Exists)
""")
        errors = checker.validate([source], self.root)
        self.assertEqual(len(errors), 5, errors)
        self.assertIn("source.md:2: broken local link: gone.md#even-with-fragment", errors[0])
        self.assertIn("missing Markdown fragment: other.md#absent", errors[1])
        self.assertIn("missing Markdown fragment: #absent", errors[2])
        self.assertIn("broken local link: missing.png", errors[3])
        self.assertIn("missing Markdown fragment: other.md#Exists", errors[4])

    def test_fenced_inline_and_comment_examples_are_not_links_or_anchors(self):
        source = self.write("README.md", """# Real
````markdown
[example](missing.md)
# Fake
```
[still example](missing.md)
````
~~~markdown
[example](missing.md)
<a id="fake-anchor"></a>
~~~
`[inline](missing.md)`
<!-- [comment](missing.md)
# Comment heading
-->
[real](#fake)
[real](#fake-anchor)
[real](#comment-heading)
[real](missing.md)
""")
        errors = checker.validate([source], self.root)
        self.assertEqual(len(errors), 4, errors)
        self.assertIn(":16: missing Markdown fragment: #fake", errors[0])
        self.assertIn(":19: broken local link: missing.md", errors[3])
        self.assertEqual(checker.heading_ids(source.read_text(encoding="utf-8")), {"real"})

    def test_language_policy_still_includes_code(self):
        source = self.write("README.md", "```text\n\u4e2d\n```\n")
        self.assertEqual(checker.validate([source], self.root), ["README.md:2: non-English Han character"])

    def test_unreadable_markdown_target_does_not_pass_fragment_check(self):
        target = self.root / "bad.md"
        target.write_bytes(b"\xff")
        source = self.write("README.md", "[target](bad.md#heading)\n")
        errors = checker.validate([source], self.root)
        self.assertEqual(len(errors), 1)
        self.assertIn("cannot check local link bad.md#heading", errors[0])

    def test_unicode_punctuation_duplicate_headings_and_html_ids(self):
        self.assertEqual(
            checker.heading_ids("# Café: `api_name()`!\n# Café: `api_name()`!\n# Hello _World_ ###\n\nOther Title\n===\n\n<span id='Exact.ID'></span>\n"),
            {"café-api_name", "café-api_name-1", "hello-world", "other-title", "Exact.ID"},
        )

    def test_help_describes_scope_without_enumerating_files(self):
        stdout = io.StringIO()
        with patch.object(checker, "markdown_files", side_effect=AssertionError("help must not enumerate")), contextlib.redirect_stdout(stdout):
            with self.assertRaises(SystemExit) as result:
                checker.main(["--help"])
        self.assertEqual(result.exception.code, 0)
        self.assertIn("nonignored untracked", stdout.getvalue())
        self.assertIn("not the staged snapshot", stdout.getvalue())
        self.assertIn("No network", " ".join(stdout.getvalue().split()))


if __name__ == "__main__":
    unittest.main()
