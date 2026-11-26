"""Unit and Git-integration tests for scripts/ci/check_layout.py."""

import contextlib
import importlib.util
import io
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1] / "ci" / "check_layout.py"
spec = importlib.util.spec_from_file_location("check_layout", PATH)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class LayoutValidationTests(unittest.TestCase):
    def test_clean_repository_layout_passes(self):
        paths = [
            "Cargo.toml",
            "README.md",
            ".github/workflows/windows-quality-gates.yml",
            ".grimoire/CONTEXT.md",
            "src/main.rs",
            "src/shell.rs",
            "src/shell/tests.rs",
            "src/backdrop/opaque_backend_tests.rs",
            "tests/support/gpu.rs",
            "crates/harbor-widget/src/lib.rs",
            "crates/harbor-widget/src/layout.rs",
            "crates/harbor-widget/src/layout/tests.rs",
            "crates/harbor-widget/tests/macro_tests.rs",
            "crates/harbor-terminal/src/io/allocation_tests.rs",
            "docs/README.md",
            "docs/widget/flex-layout.md",
            "scripts/README.md",
            "scripts/ci/check_layout.py",
        ]
        self.assertEqual(checker.validate_paths(paths), [])

    def test_rejects_disallowed_top_level_directory(self):
        errors = checker.validate_paths(["src/main.rs", "scratch/notes.txt"])
        self.assertEqual(len(errors), 1)
        self.assertIn("[top-level-dir]", errors[0])
        self.assertIn("scratch/notes.txt", errors[0])

    def test_rejects_mod_rs_in_root_or_workspace_crates(self):
        errors = checker.validate_paths(
            [
                "src/shell/mod.rs",
                "crates/example/src/nested/deep/mod.rs",
            ]
        )
        self.assertEqual(len(errors), 2)
        self.assertIn("[mod-rs]", errors[0])
        self.assertIn("crates/example/src/nested/deep/mod.rs", errors[0])
        self.assertIn("[mod-rs]", errors[1])
        self.assertIn("src/shell/mod.rs", errors[1])

    def test_rejects_flat_tests_at_src_roots_but_allows_nested_tests(self):
        errors = checker.validate_paths(
            [
                "src/example_tests.rs",
                "crates/example/src/example_tests.rs",
                "src/shell/nested_tests.rs",
                "crates/example/src/sub/nested_tests.rs",
                "crates/example/tests/integration_tests.rs",
            ]
        )
        self.assertEqual(len(errors), 2)
        self.assertIn("[flat-tests]", errors[0])
        self.assertIn("crates/example/src/example_tests.rs", errors[0])
        self.assertIn("[flat-tests]", errors[1])
        self.assertIn("src/example_tests.rs", errors[1])

    def test_rejects_stray_root_files_in_docs_and_scripts(self):
        errors = checker.validate_paths(
            [
                "docs/README.md",
                "docs/stray.md",
                "docs/widget/flex-layout.md",
                "scripts/README.md",
                "scripts/stray.py",
                "scripts/ci/check_layout.py",
            ]
        )
        self.assertEqual(len(errors), 2)
        self.assertIn("[root-readme-only]", errors[0])
        self.assertIn("docs/stray.md", errors[0])
        self.assertIn("[root-readme-only]", errors[1])
        self.assertIn("scripts/stray.py", errors[1])


@unittest.skipUnless(shutil.which("git"), "Git is required for checkout layout tests")
class GitCheckoutLayoutTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True, capture_output=True)

    def write(self, rel: str, text: str = "") -> Path:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def run_main(self, *args: str) -> tuple[int, str, str]:
        stdout, stderr = io.StringIO(), io.StringIO()
        with (
            patch.object(checker, "ROOT", self.root),
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            code = checker.main(list(args))
        return code, stdout.getvalue(), stderr.getvalue()

    def test_ignores_gitignored_directories_and_rejects_working_tree_violations(self):
        self.write(".gitignore", "/docs/verification/\n/.vscode/\n")
        self.write("docs/README.md", "# Docs\n")
        self.write("scripts/README.md", "# Scripts\n")
        self.write("src/main.rs", "fn main() {}\n")
        self.write("docs/verification/local.md", "# Ignored\n")
        self.write(".vscode/settings.json", "{}\n")
        subprocess.run(
            ["git", "add", ".gitignore", "docs/README.md", "scripts/README.md", "src/main.rs"],
            cwd=self.root,
            check=True,
            capture_output=True,
        )

        code, stdout, stderr = self.run_main()
        self.assertEqual(code, 0, stderr)
        self.assertIn("passed (4 files)", stdout)

        self.write("crates/example/src/foo/mod.rs", "")
        code, _, stderr = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("crates/example/src/foo/mod.rs", stderr)

        code, _, stderr = self.run_main("--staged")
        self.assertEqual(code, 0, stderr)


if __name__ == "__main__":
    unittest.main()
