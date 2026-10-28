"""Profiling input unit tests; included by scripts/tests discovery."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "profiling"))

from unicode_diagnosis_input import CASES, generate
from workload import rows


class UnicodeDiagnosisInputs(unittest.TestCase):
    def test_original_is_exact_workload(self):
        self.assertEqual(b"".join(generate("original", 2000)), b"".join(rows("unicode", 2000, 80)))

    def test_controls_change_only_selected_members(self):
        original = b"".join(generate("original", 2000))
        self.assertEqual(b"".join(generate("no_vs16", 2000)), original.replace("\ufe0f".encode(), b""))
        self.assertEqual(b"".join(generate("wide_vs16", 2000)), original.replace("❤".encode(), "😀".encode()))
        self.assertEqual(len(original), 82000)
        self.assertEqual(len(b"".join(generate("no_vs16", 2000))), 76000)
        self.assertEqual(len(b"".join(generate("wide_vs16", 2000))), 84000)

    def test_each_case_is_finite_numbered_utf8(self):
        for case in CASES:
            with self.subTest(case=case):
                data = list(generate(case, 17))
                self.assertEqual(len(data), 17)
                self.assertTrue(data[-1].decode("utf-8").startswith("000016 "))
                self.assertTrue(all(row.endswith(b"\r\n") for row in data))

    def test_invalid_cases_and_bounds(self):
        for case, count in [("missing", 1), ("original", 0), ("original", 20_001)]:
            with self.subTest(case=case, count=count):
                with self.assertRaises(ValueError):
                    list(generate(case, count))


if __name__ == "__main__":
    unittest.main()
