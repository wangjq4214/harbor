"""Isolated tests; never invoke Cargo or use the repository's target directory."""

import contextlib
import importlib.util
import io
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "docs" / "check_examples.py"
SPEC = importlib.util.spec_from_file_location("check_examples", SCRIPT)
check_examples = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check_examples)

MANIFEST = '''[package]
name = "fixture"
edition = "2024"
[workspace]
members = ["crates/*"]
[workspace.dependencies]
harbor-widget = { path = "crates/custom-widget" }
harbor-terminal = { path = "crates/custom-terminal" }
wgpu = "30.0"
winit = { version = "0.30", features = ["rwh_06"] }
anyhow = "1.0"
pollster = { version = "1.0", default-features = false }
'''

LOCKFILE = '''version = 4
[[package]]
name = "pinned"
version = "1.0.0"
source = "registry+https://example.invalid/index"
checksum = "controlled-checksum"
'''


class CheckExamplesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="harbor docs ü ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.put("Cargo.toml", MANIFEST)
        self.put("Cargo.lock", LOCKFILE)
        # Fail closed if any test accidentally reaches a real Cargo executable.
        self.cargo_runner = mock.patch.object(check_examples.subprocess, "run")
        self.cargo_run = self.cargo_runner.start()
        self.addCleanup(self.cargo_runner.stop)
        self.cargo_run.return_value = subprocess.CompletedProcess([], 0)

    def put(self, relative, content="# Fixture\n```rust\nassert_eq!(2 + 2, 4);\n```\n"):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def invoke(self, *args):
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(check_examples, "ROOT", self.root), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = check_examples.main(list(args))
        return code, stdout.getvalue(), stderr.getvalue()

    def test_selects_only_requested_recursive_markdown_in_stable_order(self):
        widget = self.put("docs/widget/nested/z.md")
        terminal = self.put("docs/terminal/a.md")
        self.put("docs/widget/not-markdown.txt")
        self.put("docs/app/example.md")
        self.put("docs/other.md")
        widget_readme = self.put("crates/harbor-widget/README.md")
        terminal_readme = self.put("crates/harbor-terminal/README.md")
        self.assertEqual(check_examples.markdown_files(self.root), [terminal, widget])
        self.assertEqual(check_examples.markdown_files(self.root, True),
                         [terminal_readme, widget_readme, terminal, widget])

    def test_opt_in_readmes_must_exist(self):
        self.put("docs/widget/a.md")
        with self.assertRaisesRegex(ValueError, "README is missing"):
            check_examples.markdown_files(self.root, True)

    def test_manifest_is_isolated_with_real_paths_features_and_external_versions(self):
        manifest = tomllib.loads(check_examples.cargo_manifest(self.root))
        self.assertEqual(manifest["workspace"], {})
        self.assertFalse(manifest["package"]["publish"])
        self.assertEqual(manifest["package"]["edition"], "2024")
        deps = manifest["dependencies"]
        self.assertEqual(deps["harbor-widget"], {
            "path": str(self.root / "crates/custom-widget"), "features": ["winit"]})
        self.assertEqual(deps["harbor-terminal"], {"path": str(self.root / "crates/custom-terminal")})
        # Absence of default-features=false retains terminal's default renderer.
        originals = tomllib.loads(MANIFEST)["workspace"]["dependencies"]
        for name in check_examples.EXTERNALS:
            self.assertEqual(deps[name], originals[name])
        self.assertEqual(set(deps), {*check_examples.EXTERNALS, "harbor-widget", "harbor-terminal"})
        self.cargo_run.assert_not_called()

    def test_workspace_lock_is_seeded_and_refreshed_without_changing_original(self):
        self.put("docs/widget/a.md")
        self.assertEqual(self.invoke("--prepare-only")[0], 0)
        original = self.root / "Cargo.lock"
        derived = self.root / "target/docs-examples/Cargo.lock"
        self.assertEqual(derived.read_bytes(), original.read_bytes())
        original.write_bytes(b"updated workspace pins\n")
        derived.write_bytes(b"stale resolved package\n")
        self.assertEqual(self.invoke("--prepare-only")[0], 0)
        self.assertEqual(derived.read_bytes(), b"updated workspace pins\n")
        self.assertEqual(original.read_bytes(), b"updated workspace pins\n")

    def test_missing_workspace_lock_fails_before_generating(self):
        self.put("docs/widget/a.md")
        (self.root / "Cargo.lock").unlink()
        self.assertEqual(self.invoke("--prepare-only")[0], 1)
        self.assertFalse((self.root / "target/docs-examples").exists())
        self.cargo_run.assert_not_called()

    def test_external_dependency_drift_fails_even_after_successful_cargo(self):
        self.put("docs/widget/a.md")
        def drift(*args, **kwargs):
            self.put("target/docs-examples/Cargo.lock", LOCKFILE.replace('version = "1.0.0"', 'version = "2.0.0"'))
            return subprocess.CompletedProcess([], 0)
        self.cargo_run.side_effect = drift
        code, _, stderr = self.invoke()
        self.assertEqual(code, 1)
        self.assertIn("drifted outside workspace Cargo.lock: pinned 2.0.0", stderr)
        self.assertEqual((self.root / "Cargo.lock").read_text(encoding="utf-8"), LOCKFILE)

    def test_generated_local_package_does_not_count_as_external_drift(self):
        self.put("docs/widget/a.md")
        self.invoke("--prepare-only")
        lock = self.put("target/docs-examples/Cargo.lock", LOCKFILE + '\n[[package]]\nname = "harbor-doc-examples"\nversion = "0.0.0"\n')
        check_examples.verify_external_pins(self.root, lock)

    def test_toml_strings_escape_windows_paths_quotes_and_unicode(self):
        value = 'C:\\a path\\"quoted"\\é.md'
        encoded = check_examples.toml_value({"path": value, "features": ["winit"]})
        self.assertEqual(tomllib.loads("dep = " + encoded)["dep"]["path"], value)

    def test_rust_string_uses_rust_not_json_control_escapes(self):
        value = 'C:\\a path\\"é"\n\r\t\x00\x1f\x7f'
        expected = '"C:\\\\a path\\\\\\"é\\"\\n\\r\\t\\u{0}\\u{1f}\\u{7f}"'
        self.assertEqual(check_examples.rust_string(value), expected)
        # Raw string terminators in a filename cannot close this ordinary literal.
        self.assertEqual(check_examples.rust_string('a"###b'), '"a\\"###b"')

    def test_modules_include_absolute_paths_and_have_stable_unique_names(self):
        paths = [self.put("docs/widget/type.md"), self.put("docs/widget/9 a.md"),
                 self.put("docs/widget/9-a.md"), self.put("docs/terminal/type.md")]
        names = [check_examples.module_name(path.relative_to(self.root)) for path in paths]
        self.assertEqual(len(set(names)), len(paths))
        for name in names:
            self.assertRegex(name, r"^doc_[a-f0-9]+$")
        source = check_examples.library_source(self.root, paths)
        reversed_source = check_examples.library_source(self.root, list(reversed(paths)))
        for path, name in zip(paths, names):
            declaration = (f"#[doc = include_str!({check_examples.rust_string(str(path))})]\n"
                           f"pub mod {name} {{}}")
            self.assertIn(declaration, source)
            self.assertIn(declaration, reversed_source)
        # Adding an earlier file or relocating the checkout does not renumber modules.
        self.assertEqual(check_examples.module_name(Path("docs/widget/type.md")), names[0])
        self.assertEqual(len(re.findall(r"pub mod ", source)), len(paths))

    def test_prepare_only_generates_package_and_preserves_unrelated_files(self):
        first = self.put("docs/widget/a.md")
        before = (self.root / "Cargo.toml").read_bytes()
        code, stdout, stderr = self.invoke("--prepare-only")
        self.assertEqual((code, stderr), (0, ""))
        self.assertIn("examples have not been verified", stdout)
        self.cargo_run.assert_not_called()
        package = self.root / "target/docs-examples"
        self.assertEqual((package / "src/lib.rs").read_text(encoding="utf-8"),
                         check_examples.library_source(self.root, [first]))
        self.assertEqual(tomllib.loads((package / "Cargo.toml").read_text(encoding="utf-8")),
                         tomllib.loads(check_examples.cargo_manifest(self.root)))
        sentinel = self.put("target/docs-examples/other-agent.txt", "do not delete")
        lock = self.put("target/docs-examples/Cargo.lock", "reuse generated lock")
        self.put("docs/terminal/b.md", "```rust,no_run\nfn main() {}\n```\n")
        self.assertEqual(self.invoke("--prepare-only")[0], 0)
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "do not delete")
        self.assertEqual(lock.read_bytes(), (self.root / "Cargo.lock").read_bytes())
        self.assertEqual((self.root / "Cargo.toml").read_bytes(), before)
        self.assertEqual((self.root / "Cargo.lock").read_text(encoding="utf-8"),
                         LOCKFILE)
        source = (package / "src/lib.rs").read_text(encoding="utf-8")
        self.assertEqual(source.count("pub mod "), 2)

    def test_no_markdown_fails_without_cargo_or_generated_package(self):
        for args in ((), ("--check",), ("--prepare-only",)):
            with self.subTest(args=args):
                code, _, stderr = self.invoke(*args)
                self.assertEqual(code, 1)
                self.assertIn("No Markdown selected", stderr)
        self.cargo_run.assert_not_called()
        self.assertFalse((self.root / "target").exists())
        with self.assertRaisesRegex(ValueError, "No Markdown selected"):
            check_examples.prepare(self.root, [])

    def test_refuses_foreign_generated_directory_without_modifying_it(self):
        self.put("docs/widget/a.md")
        existing = self.put("target/docs-examples/Cargo.toml", "foreign manifest")
        code, _, stderr = self.invoke("--prepare-only")
        self.assertEqual(code, 1)
        self.assertIn("non-owned generated directory", stderr)
        self.assertEqual(existing.read_text(encoding="utf-8"), "foreign manifest")
        self.cargo_run.assert_not_called()
        self.assertFalse((existing.parent / "src").exists())

    def test_default_and_check_execute_cargo_with_explicit_shared_cache(self):
        self.put("docs/widget/a.md")
        for args in ((), ("--check",)):
            with self.subTest(args=args):
                self.cargo_run.reset_mock()
                code, stdout, stderr = self.invoke(*args)
                self.assertEqual((code, stderr), (0, ""))
                self.assertIn("no_run compiles only", stdout)
                self.assertIn("ignored examples are not validated", stdout)
                self.cargo_run.assert_called_once_with(
                    ["cargo", "test", "--doc", "--manifest-path",
                     str(self.root / "target/docs-examples/Cargo.toml"),
                     "--target-dir", str(self.root / "target")], cwd=self.root, check=False)

    def test_cargo_failure_exit_code_is_propagated(self):
        self.put("docs/terminal/a.md")
        self.cargo_run.return_value = subprocess.CompletedProcess([], 23)
        code, _, stderr = self.invoke("--check")
        self.assertEqual(code, 23)
        self.assertIn("Cargo/Rustdoc failed (exit 23)", stderr)

    def test_missing_cargo_reports_failure(self):
        self.put("docs/widget/a.md")
        self.cargo_run.side_effect = FileNotFoundError("cargo unavailable")
        code, _, stderr = self.invoke()
        self.assertEqual(code, 1)
        self.assertIn("cargo unavailable", stderr)

    def test_invalid_workspace_manifest_fails_before_cargo(self):
        self.put("docs/widget/a.md")
        self.put("Cargo.toml", "[not valid toml")
        code, _, stderr = self.invoke()
        self.assertEqual(code, 1)
        self.assertIn("Example verification failed", stderr)
        self.cargo_run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
