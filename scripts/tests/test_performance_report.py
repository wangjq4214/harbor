"""Advisory A/B report and trusted sticky-comment tests; no network/GPU required."""

import argparse
import contextlib
import copy
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import urllib.request
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "ci"))
import performance_report as report
import publish_performance_comment as publisher


def measurement_output(stage, value=100):
    count = 101 if stage == "prepare" else 11
    return "\n".join(report.PREFIX + json.dumps({"case": case, "samples_ns": [value] * count})
                     for case in report.STAGES[stage])


def sample_report():
    return {
        "schema": 1, "pr": 7, "run_id": 91, "run_number": 5, "attempt": 1,
        "base_sha": "1" * 40, "head_sha": "2" * 40, "merge_sha": "3" * 40,
        "probe_sha256": "a" * 64, "rounds": 2,
        "environment": {"os": "Windows", "cpu": "CPU", "rustc": "rustc 1.99"},
        "results": {
            case: {label: {"error": None, "samples": [[value] * (101 if case in report.PREPARE else 11)
                                                     for _ in range(2)]}
                   for label, value in (("base", 100), ("candidate", 120))}
            for case in report.CASES
        },
    }


def archive(data):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", zipfile.ZIP_DEFLATED) as zipped:
        zipped.writestr("report.json", json.dumps(data))
    return stream.getvalue()


class FakeGitHub:
    repo = "owner/harbor"

    def __init__(self):
        self.data = sample_report()
        self.run = {
            "id": 91, "run_number": 5, "run_attempt": 1, "event": "pull_request",
            "status": "completed", "head_sha": "2" * 40,
            "repository": {"full_name": self.repo}, "pull_requests": [{"number": 7}],
        }
        self.pr = {
            "number": 7, "state": "open", "head": {"sha": "2" * 40},
            "base": {"sha": "1" * 40, "repo": {"full_name": self.repo}},
            "merge_commit_sha": "3" * 40,
        }
        self.comments = []
        self.writes = []
        self.raw = None
        self.no_artifact = False
        self.reads = 0
        self.advance_on_read = None

    def request(self, path, method="GET", data=None, binary=False):
        if path == "/actions/runs/91":
            return copy.deepcopy(self.run)
        if path == "/pulls/7":
            self.reads += 1
            if self.reads == self.advance_on_read:
                self.pr["head"]["sha"] = "4" * 40
            return copy.deepcopy(self.pr)
        if path == "/actions/artifacts/6/zip":
            return self.raw if self.raw is not None else archive(self.data)
        if method in ("POST", "PATCH"):
            self.writes.append((path, method, data))
            if method == "POST":
                self.comments.append({
                    "id": 100, "user": {"login": "github-actions[bot]", "type": "Bot"},
                    "body": data["body"],
                })
            else:
                comment = next(c for c in self.comments if path.endswith("/" + str(c["id"])))
                comment["body"] = data["body"]
            return {}
        raise AssertionError(f"unexpected request {path} {method}")

    def pages(self, path, key=None):
        if path == "/actions/runs/91/artifacts":
            return [] if self.no_artifact else [{
                "id": 6, "name": f"pr-performance-data-{self.run['run_attempt']}",
                "size_in_bytes": 100, "expired": False,
            }]
        if path == "/issues/7/comments":
            return copy.deepcopy(self.comments)
        if path.startswith("/commits/"):
            return [{"number": 7}]
        raise AssertionError(f"unexpected pages {path}")

    def publish(self, attempt=None):
        with contextlib.redirect_stdout(io.StringIO()):
            publisher.publish(self, {"workflow_run": {
                "id": 91, "run_attempt": attempt or self.run["run_attempt"],
            }})


class ReportTests(unittest.TestCase):
    def test_parse_all_expected_cases_and_ignore_diagnostic_lines(self):
        for stage in report.STAGES:
            rows = report.parse_output("adapter diagnostic\n" + measurement_output(stage), stage)
            self.assertEqual(set(rows), set(report.STAGES[stage]))

    def test_parse_rejects_empty_partial_duplicate_and_unknown_measurements(self):
        valid = measurement_output("core")
        first = valid.splitlines()[0]
        for value in ("", "noise only", first, valid + "\n" + first,
                      valid.replace("ingest_ascii", "unknown"), report.PREFIX + "[]"):
            with self.subTest(value=value[:50]), self.assertRaises(ValueError):
                report.parse_output(value, "core")

    def test_invalid_samples_never_become_zero_change(self):
        for invalid in (True, 0, -1, float("inf"), float("nan"), "100", None, 1e16, 10**400):
            data = sample_report()
            data["results"]["ingest_ascii"]["base"]["samples"][0][0] = invalid
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                report.validate_report(data)
        lines = measurement_output("core").splitlines()
        for samples in ([], [100], [100] * 10, [100] * 12, [float("nan")] * 11):
            row = {"case": "ingest_ascii", "samples_ns": samples}
            with self.subTest(samples=samples[:1]), self.assertRaises(ValueError):
                report.parse_output(report.PREFIX + json.dumps(row) + "\n" + "\n".join(lines[1:]), "core")

    def test_schema_and_provenance_are_bounded(self):
        for key, value in (("schema", True), ("rounds", 1), ("rounds", 9), ("pr", True),
                           ("base_sha", "bad"), ("probe_sha256", "bad")):
            data = sample_report()
            data[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                report.validate_report(data)
        data = sample_report()
        del data["results"]["ingest_ascii"]
        with self.assertRaises(ValueError):
            report.validate_report(data)
        for error in ([], {}, 1, "made up"):
            data = sample_report()
            data["results"]["ingest_ascii"]["base"]["error"] = error
            with self.assertRaises(ValueError):
                report.validate_report(data)

    def test_changes_are_advisory_with_explicit_direction_and_noise(self):
        text = report.render_report(sample_report())
        self.assertIn("+20.0%", text)
        self.assertIn("do not block merging", text)
        self.assertIn("Positive time change = slower", text)
        self.assertIn("not confirmed regressions", text)
        self.assertIn("not input/frame tail latency", text)
        self.assertIn("Between-process median ranges", text)
        data = sample_report()
        data["results"]["ingest_ascii"]["candidate"]["samples"] = [[80] * 11] * 2
        self.assertIn("-20.0%", report.render_report(data))

    def test_distribution_uses_process_medians_not_a_favorable_single_run(self):
        median, p95, low, high = report.distribution([[1, 2, 3], [10, 20, 30]])
        self.assertEqual((median, p95, low, high), (11, 16.5, 2, 20))

    def test_partial_report_is_explicitly_unavailable_and_not_silently_omitted(self):
        data = sample_report()
        data["results"]["ingest_ascii"]["candidate"] = {"error": "measurement failed", "samples": []}
        text = report.render_report(data)
        self.assertIn("Unavailable (candidate: measurement failed)", text)
        self.assertIn("Unicode output", text)
        data["results"]["ingest_ascii"]["candidate"]["samples"] = [[100] * 11]
        with self.assertRaises(ValueError):
            report.validate_report(data)

    def test_artifact_text_cannot_inject_markdown_mentions_or_links(self):
        data = sample_report()
        data["environment"]["cpu"] = "@everyone\n| [link](https://evil.test) | <script>"
        data["body"] = "@everyone malicious supplied Markdown"
        text = report.render_report(data)
        self.assertNotIn("@everyone", text)
        self.assertNotIn("[link]", text)
        self.assertNotIn("<script>", text)
        self.assertNotIn("supplied Markdown", text)

    def test_build_copies_identical_probe_and_restores_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            example = root / "crates/harbor-terminal/examples/pr_performance.rs"
            example.parent.mkdir(parents=True)
            out = root / "out"
            out.mkdir()
            probe = b"the same probe"
            seen = []

            def compile_probe(*args, **kwargs):
                seen.append(example.read_bytes())
                source = out / "build/release/examples"
                source.mkdir(parents=True, exist_ok=True)
                (source / ("pr_performance.exe" if os.name == "nt" else "pr_performance")).write_bytes(b"binary")
                (source / "conpty.dll").write_bytes(b"dll")

            with mock.patch.object(report, "command", side_effect=compile_probe):
                before = report.build(root, out, "base", probe)
                self.assertFalse(example.exists())
                example.write_bytes(b"preexisting")
                after = report.build(root, out, "candidate", probe)
                self.assertEqual(example.read_bytes(), b"preexisting")
                self.assertEqual(seen, [probe, probe])
                self.assertNotEqual(before.parent, after.parent)
                self.assertTrue((before.parent / "conpty.dll").exists())
                self.assertTrue((after.parent / "conpty.dll").exists())
            with mock.patch.object(report, "command", side_effect=RuntimeError("compile error")):
                with self.assertRaises(RuntimeError):
                    report.build(root, out, "base", probe)
                self.assertEqual(example.read_bytes(), b"preexisting")

    def run_measure(self, directory, *, fail=None, interrupt=False):
        root = Path(directory)
        event = root / "event.json"
        event.write_text(json.dumps({"number": 7, "pull_request": {
            "base": {"sha": "1" * 40}, "head": {"sha": "2" * 40},
        }}), encoding="utf-8")
        args = argparse.Namespace(event=event, base=root / "base", candidate=root / "candidate",
                                  out=root / "out", rounds=2)
        order = []

        def command(arguments, **kwargs):
            if arguments == ["rustc", "--version"]:
                return "rustc test"
            if arguments[:2] == ["git", "rev-parse"]:
                return ("1" if kwargs["cwd"].name == "base" else "3") * 40
            label, stage = Path(arguments[0]).name, arguments[1]
            order.append((label, stage))
            if interrupt and stage == "prepare":
                raise KeyboardInterrupt()
            if fail == (label, stage):
                raise RuntimeError("timeout")
            return measurement_output(stage)

        env = {"GITHUB_RUN_ID": "91", "GITHUB_RUN_NUMBER": "5",
               "GITHUB_RUN_ATTEMPT": "1", "GITHUB_SHA": "3" * 40, "GITHUB_STEP_SUMMARY": ""}
        with mock.patch.dict(os.environ, env), mock.patch.object(report, "command", side_effect=command), \
                mock.patch.object(report, "build", side_effect=lambda root, out, label, probe: out / label), \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            if interrupt:
                with self.assertRaises(KeyboardInterrupt):
                    report.measure(args)
            else:
                self.assertIsNone(report.measure(args))  # no regression-based exit failure
        result = json.loads((args.out / "report.json").read_text(encoding="utf-8"))
        report.validate_report(result)
        return result, order

    def test_measure_builds_before_serial_alternating_runs(self):
        with tempfile.TemporaryDirectory() as directory:
            result, order = self.run_measure(directory)
            self.assertEqual(order[:4], [("base", "core"), ("candidate", "core"),
                                         ("candidate", "core"), ("base", "core")])
            self.assertTrue(all(row["error"] is None for labels in result["results"].values()
                                for row in labels.values()))

    def test_measure_failure_does_not_discard_other_stage_or_block(self):
        with tempfile.TemporaryDirectory() as directory:
            result, _ = self.run_measure(directory, fail=("candidate", "prepare"))
            self.assertIsNone(result["results"]["ingest_ascii"]["candidate"]["error"])
            self.assertEqual(result["results"]["prepare_emoji_local"]["candidate"]["error"],
                             "measurement failed")

    def test_interrupted_report_retains_only_completed_stages(self):
        with tempfile.TemporaryDirectory() as directory:
            result, _ = self.run_measure(directory, interrupt=True)
            self.assertIsNone(result["results"]["ingest_ascii"]["candidate"]["error"])
            self.assertEqual(result["results"]["prepare_emoji_local"]["base"],
                             {"samples": [], "error": "not measured"})


class PublisherTests(unittest.TestCase):
    def test_first_run_creates_one_comment_then_updates_it(self):
        api = FakeGitHub()
        api.publish()
        api.publish()
        self.assertEqual([method for _, method, _ in api.writes], ["POST", "PATCH"])
        self.assertEqual(len(api.comments), 1)
        self.assertIn("+20.0%", api.comments[0]["body"])
        self.assertIn("/actions/runs/91/attempts/1", api.comments[0]["body"])

    def test_rerun_updates_same_comment(self):
        api = FakeGitHub()
        api.publish()
        api.run["run_attempt"] = api.data["attempt"] = 2
        api.publish()
        self.assertEqual(len(api.comments), 1)
        self.assertIn("attempt=2", api.comments[0]["body"])

    def test_human_marker_is_not_edited(self):
        api = FakeGitHub()
        api.comments.append({"id": 1, "user": {"login": "person", "type": "User"},
                             "body": "<!-- harbor-performance:v1 run=91 number=5 attempt=1 -->"})
        api.publish()
        self.assertEqual(api.writes[0][1], "POST")

    def test_older_run_or_attempt_cannot_overwrite_newer_report(self):
        for number, attempt in ((6, 1), (5, 2)):
            api = FakeGitHub()
            api.comments.append({
                "id": 99, "user": {"login": "github-actions[bot]", "type": "Bot"},
                "body": f"<!-- harbor-performance:v1 run=92 number={number} attempt={attempt} -->new",
            })
            api.publish()
            self.assertEqual(api.writes, [])

    def test_no_publication_for_stale_or_wrong_provenance(self):
        for key, value in (("head_sha", "4" * 40), ("base_sha", "4" * 40),
                           ("merge_sha", "4" * 40), ("run_id", 92), ("pr", 8),
                           ("attempt", 2), ("run_number", 6)):
            api = FakeGitHub()
            api.data[key] = value
            api.publish()
            self.assertEqual(api.writes, [], key)

    def test_closed_or_advanced_pr_and_superseded_attempt_are_skipped(self):
        for setup in ("closed", "advanced", "attempt", "event", "repository"):
            api = FakeGitHub()
            if setup == "closed":
                api.pr["state"] = "closed"
            elif setup == "advanced":
                api.pr["head"]["sha"] = "4" * 40
            elif setup == "event":
                api.run["event"] = "push"
            elif setup == "repository":
                api.run["repository"]["full_name"] = "other/repo"
            else:
                api.run["run_attempt"] = 2
            api.publish(attempt=1)
            self.assertEqual(api.writes, [], setup)

    def test_pr_advancing_during_publication_is_rechecked(self):
        api = FakeGitHub()
        api.advance_on_read = 2
        api.publish()
        self.assertEqual(api.writes, [])

    def test_fork_missing_run_association_uses_authenticated_commit_association(self):
        api = FakeGitHub()
        api.run["pull_requests"] = []
        api.publish()
        self.assertEqual(api.writes[0][1], "POST")

    def test_missing_or_invalid_artifact_reports_unavailable_not_zero_change(self):
        for raw in (None, b"not a zip", archive({}), archive({"schema": 1})):
            api = FakeGitHub()
            if raw is None:
                api.no_artifact = True
            else:
                api.raw = raw
            api.publish()
            text = api.writes[0][2]["body"]
            self.assertIn("Measurements unavailable", text)
            self.assertNotIn("+0.0%", text)
            self.assertNotIn("+20.0%", text)

    def test_zip_path_traversal_extra_files_and_expansion_are_rejected(self):
        for name, payload, extra in (("../report.json", "{}", False),
                                     ("report.json", "{}", True),
                                     ("report.json", " " * (publisher.MAX_BYTES + 1), False)):
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, "w", zipfile.ZIP_DEFLATED) as zipped:
                zipped.writestr(name, payload)
                if extra:
                    zipped.writestr("script.py", "raise Exception()")
            with self.assertRaises(ValueError):
                publisher.load_artifact(stream.getvalue())

    def test_redirect_never_forwards_token_to_storage_host(self):
        handler = publisher.SafeRedirect()
        request = urllib.request.Request("https://api.github.com/artifact",
                                         headers={"Authorization": "Bearer secret"})
        redirected = handler.redirect_request(request, None, 302, "", {},
                                              "https://storage.example/artifact.zip")
        self.assertFalse(redirected.has_header("Authorization"))
        with self.assertRaises(ValueError):
            handler.redirect_request(request, None, 302, "", {}, "http://api.github.com/artifact")


class WorkflowBoundaryTests(unittest.TestCase):
    def test_performance_is_separate_from_required_quality(self):
        root = Path(__file__).resolve().parents[2]
        quality = (root / ".github/workflows/windows-quality-gates.yml").read_text(encoding="utf-8")
        self.assertIn("needs: [quick, rust, package]", quality)
        measurement = (root / ".github/workflows/pr-performance.yml").read_text(encoding="utf-8")
        self.assertNotIn("pull-requests: write", measurement)
        self.assertNotIn("pull_request_target", measurement)
        self.assertIn("name: pr-performance-data-${{ github.run_attempt }}", measurement)
        publication = (root / ".github/workflows/pr-performance-comment.yml").read_text(encoding="utf-8")
        self.assertIn("workflow_run:", publication)
        self.assertIn("ref: ${{ github.event.repository.default_branch }}", publication)
        self.assertNotIn("head_sha }}", publication)
        self.assertIn("cancel-in-progress: false", publication)


if __name__ == "__main__":
    unittest.main()
