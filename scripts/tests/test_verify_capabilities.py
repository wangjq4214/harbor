"""Portable fixture tests; mocked I/O is not ConPTY/SSH acceptance."""
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1] / "terminal" / "verify_capabilities.py"
spec = importlib.util.spec_from_file_location("capability_probe", PATH)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class CapabilityProbeTests(unittest.TestCase):
    def test_query_names_preserve_case(self):
        self.assertEqual(probe.tcap(["Co", "colors", "U8", "u8"]),
                         b"\x1bP+q436f;636f6c6f7273;5538;7538\x1b\\")

    def test_incomplete_csi_dcs_and_ss3_are_not_complete_frames(self):
        for data in [b"", b"\x1b", b"\x1b[", b"\x1b[?1;2$", b"\x1bO",
                     b"\x1bP1+r5538=31", b"\x1bP1+r5538=31\x1b"]:
            with self.subTest(data=data):
                self.assertFalse(probe.complete(data))

    def test_read_reply_accepts_every_split_and_one_byte_chunks(self):
        for frame in [b"\x1b[?62;6;17;22;28c", b"\x1b[?1;2$y", b"\x1bOH",
                      b"\x1bP1+r5538=31\x1b\\", b"\x1bP1$r0m\x1b\\"]:
            splits = [[frame[:i], frame[i:]] for i in range(1, len(frame))]
            splits.append([bytes([byte]) for byte in frame])
            for chunks in splits:
                with self.subTest(frame=frame, chunks=chunks):
                    with patch.object(probe.select, "select", return_value=([7], [], [])), \
                         patch.object(probe.os, "read", side_effect=chunks):
                        self.assertEqual(probe.read_reply(7, 1), frame)

    def test_timeout_eof_and_retention_bound(self):
        with patch.object(probe.select, "select", return_value=([], [], [])):
            self.assertEqual(probe.read_reply(7, 0.1), b"")
        with patch.object(probe.select, "select", return_value=([7], [], [])), \
             patch.object(probe.os, "read", return_value=b""):
            self.assertEqual(probe.read_reply(7, 0.1), b"")
        with patch.object(probe.select, "select", return_value=([7], [], [])), \
             patch.object(probe.os, "read", return_value=b"\x1bP" + b"x" * 4094) as read:
            self.assertEqual(len(probe.read_reply(7, 0.1)), 4096)
            read.assert_called_once_with(7, 4096)

    def test_tmux_layer_requires_and_records_the_active_pane(self):
        with patch.dict(probe.os.environ, {}, clear=True):
            with self.assertRaisesRegex(RuntimeError, "active tmux pane"):
                probe.tmux_context("tmux")
        with patch.dict(probe.os.environ, {"TMUX": "test-socket,123,0", "TMUX_PANE": "%1"}), \
             patch.object(probe, "command_version", side_effect=["%1", "xterm-256color", "256,RGB", "tmux-256color"]):
            self.assertEqual(probe.tmux_context("tmux"), {
                "pane": "%1", "outer_term": "xterm-256color", "client_features": "256,RGB",
                "default_terminal": "tmux-256color"})
        with patch.dict(probe.os.environ, {"TMUX": "test-socket,123,0", "TMUX_PANE": "%1"}), \
             patch.object(probe, "command_version", return_value="unavailable"):
            with self.assertRaisesRegex(RuntimeError, "verify the active"):
                probe.tmux_context("tmux")
        self.assertIsNone(probe.tmux_context("harbor"))

    def test_vim_errors_are_failures_not_skips(self):
        for error in [FileNotFoundError("vim"), subprocess.TimeoutExpired("vim", 45)]:
            with patch.object(probe.subprocess, "run", side_effect=error):
                result = probe.vim_workload()
            self.assertEqual(result[0]["status"], "FAIL")

    def test_infrastructure_failure_writes_report_and_failure_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "result.json"
            with patch("sys.argv", [str(PATH), "--path", "test-only", "--output", str(output)]), \
                 patch.object(probe, "run", side_effect=RuntimeError("no tty")), \
                 patch("sys.stdout", new_callable=io.StringIO) as stdout:
                self.assertEqual(probe.main(), 1)
                self.assertIn("HARBOR_PROBE_DONE:FAIL", stdout.getvalue())
            report = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "FAIL")
            self.assertEqual(report["cases"][0]["error"], "no tty")


if __name__ == "__main__":
    unittest.main()
