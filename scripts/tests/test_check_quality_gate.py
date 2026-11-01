"""Exercise the final Actions gate without GitHub, Cargo or native resources."""

import contextlib
import importlib.util
import io
import itertools
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "ci" / "check_quality_gate.py"
SPEC = importlib.util.spec_from_file_location("check_quality_gate", SCRIPT)
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


def payload(**changes):
    jobs = {job: {"result": "success", "outputs": {}} for job in gate.REQUIRED_JOBS}
    jobs.update(changes)
    return json.dumps(jobs)


class QualityGateTests(unittest.TestCase):
    def invoke(self, raw, summary_path=""):
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.dict(os.environ, {
            "NEEDS_JSON": raw, "GITHUB_STEP_SUMMARY": summary_path,
        }), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = gate.main()
        return code, stdout.getvalue(), stderr.getvalue()

    def test_only_all_success_combinations_pass(self):
        for results in itertools.product(
                ("success", "failure", "cancelled", "skipped"), repeat=3):
            with self.subTest(results=results):
                raw = payload(**{job: {"result": result}
                                for job, result in zip(gate.REQUIRED_JOBS, results)})
                passed, summary = gate.evaluate(raw)
                self.assertEqual(passed, all(result == "success" for result in results))
                for job, result in zip(gate.REQUIRED_JOBS, results):
                    self.assertIn(f"| {job} | {result} |", summary)

    def test_missing_or_malformed_required_job_fails_closed(self):
        for job in gate.REQUIRED_JOBS:
            for details in (None, {}, [], "success", {"result": None},
                            {"result": True}, {"result": []}, {"result": {}},
                            {"result": "unknown"}, {"result": "success\n| injected |"}):
                with self.subTest(job=job, details=details):
                    self.assertFalse(gate.evaluate(payload(**{job: details}))[0])
            jobs = json.loads(payload())
            del jobs[job]
            self.assertFalse(gate.evaluate(json.dumps(jobs))[0])
        self.assertFalse(gate.evaluate("{}")[0])

    def test_extra_success_cannot_mask_a_required_failure(self):
        passed, _ = gate.evaluate(payload(rust={"result": "failure"},
                                          unrelated={"result": "success"}))
        self.assertFalse(passed)

    def test_invalid_json_or_top_level_shape_returns_failure(self):
        for raw in ("", "{", "null", "[]", "true", '"success"'):
            with self.subTest(raw=raw):
                code, stdout, stderr = self.invoke(raw)
                self.assertEqual(code, 1)
                self.assertEqual(stdout, "")
                self.assertIn("Invalid quality gate input", stderr)

    def test_success_appends_summary_and_failure_is_actionable(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "summary.md"
            path.write_text("Previous step\n", encoding="utf-8")
            code, stdout, stderr = self.invoke(payload(), str(path))
            self.assertEqual((code, stderr), (0, ""))
            self.assertEqual(path.read_text(encoding="utf-8"), "Previous step\n" + stdout)
            code, stdout, stderr = self.invoke(payload(package={"result": "skipped"}),
                                              str(path))
            self.assertEqual(code, 1)
            self.assertIn("| package | skipped |", stdout)
            self.assertIn("Every required lane must succeed", stderr)

    def test_cli_exit_status_includes_missing_environment(self):
        env = {key: value for key, value in os.environ.items()
               if key not in ("NEEDS_JSON", "GITHUB_STEP_SUMMARY")}
        for raw, expected in ((payload(), 0), (payload(quick={"result": "failure"}), 1),
                              (None, 1)):
            with self.subTest(raw=raw):
                case_env = dict(env)
                if raw is not None:
                    case_env["NEEDS_JSON"] = raw
                result = subprocess.run([sys.executable, "-B", str(SCRIPT)], env=case_env,
                                        capture_output=True, text=True, check=False)
                self.assertEqual(result.returncode, expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
