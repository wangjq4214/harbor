"""Run with python -m unittest discover -s scripts/profiling -p 'test_*.py'."""

import hashlib
import subprocess
import sys
import unittest
from pathlib import Path

import workload


class WorkloadTests(unittest.TestCase):
    def test_manifest_matches_exact_bytes(self):
        for mode in workload.MODES:
            with self.subTest(mode=mode):
                data = b"".join(workload.rows(mode, 29, 80))
                manifest = workload.describe(mode, 29, 80)
                self.assertEqual(manifest["bytes"], len(data))
                self.assertEqual(manifest["sha256"], hashlib.sha256(data).hexdigest())
                self.assertEqual(data, b"".join(workload.rows(mode, 29, 80)))
                self.assertIn("000028", data.decode("utf-8"))

    def test_unicode_controls_and_dirty_boundary(self):
        text = next(workload.rows("unicode", 1, 80)).decode("utf-8")
        for control in ("e\u0301", "\u2764\ufe0f", "👩\u200d💻", "界", "😀"):
            self.assertIn(control, text)
        dirty = list(workload.rows("dirty", 25, 80))
        self.assertTrue(dirty[23].startswith(b"\x1b[24;1H"))
        self.assertTrue(dirty[24].startswith(b"\x1b[1;1H"))
        self.assertNotIn(b"\n", b"".join(dirty))

    def test_cli_uses_binary_utf8_without_newline_translation(self):
        result = subprocess.run(
            [sys.executable, str(Path(workload.__file__)), "--mode", "unicode", "--rows", "2"],
            capture_output=True, check=True,
        )
        self.assertEqual(result.stdout, b"".join(workload.rows("unicode", 2, 80)))

    def test_bounds_reject_unbounded_or_unknown_input(self):
        for mode, count, columns in [
            ("unknown", 1, 80),
            ("latin", 0, 80),
            ("latin", 200_001, 80),
            ("latin", 1, 15),
            ("latin", 1, 501),
        ]:
            with self.subTest(mode=mode, count=count, columns=columns):
                with self.assertRaises(ValueError):
                    list(workload.rows(mode, count, columns))


if __name__ == "__main__":
    unittest.main()
