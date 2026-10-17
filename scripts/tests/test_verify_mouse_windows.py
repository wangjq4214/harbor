"""Portable protocol/logging tests; mocked I/O is not Windows runtime acceptance."""
import importlib.util
import io
import json
from pathlib import Path
import queue
import tempfile
import threading
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1] / "terminal" / "verify_mouse_windows.py"
spec = importlib.util.spec_from_file_location("mouse_probe", PATH)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class FakeConsole:
    def __init__(self):
        self.written = []
        self.inbox = queue.Queue()
        self.inbox.put(b"q")
        self.closed = False

    def start(self):
        pass

    def write(self, data):
        self.written.append(data)

    def close(self):
        self.closed = True


class ProtocolTests(unittest.TestCase):
    def frames(self, chunks, encoding=0):
        framer, frames = probe.Framer(), []
        for chunk in chunks:
            framer.pending.extend(chunk)
            while (raw := framer.pop(encoding)) is not None:
                frames.append(raw)
        self.assertFalse(framer.pending)
        return frames

    def test_exact_formats_and_release(self):
        for encoding, raw, action in [
            (0, b"\x1b[M\x20\x23\x22", "press"),
            (0, b"\x1b[M\x23\x23\x22", "release"),
            (1005, b"\x1b[M\x20\x23\x22", "press"),
            (1015, b"\x1b[32;3;2M", "press"),
            (1015, b"\x1b[35;3;2M", "release"),
            (1006, b"\x1b[<0;3;2M", "press"),
            (1006, b"\x1b[<0;3;2m", "release"),
            (1016, b"\x1b[<0;3;2m", "release"),
        ]:
            decoded = probe.decode_report(raw, encoding)
            self.assertEqual((decoded["x"], decoded["y"], decoded["action"]), (3, 2, action))

    def test_every_split_and_bytewise_framing(self):
        for encoding, report in [(0, b"\x1b[M\x20\xff\xff"),
                                 (1005, b"\x1b[M\x20\xdf\xbf\xc2\x80"),
                                 (1015, b"\x1b[32;128;2M"),
                                 (1006, b"\x1b[<66;3;2M")]:
            stream = report + b"\x1b[?1005;1$y" + b"q"
            expected = [report, b"\x1b[?1005;1$y", b"q"]
            for split in range(len(stream) + 1):
                self.assertEqual(self.frames([stream[:split], stream[split:]], encoding), expected)
            self.assertEqual(self.frames([bytes([b]) for b in stream], encoding), expected)

    def test_mouse_fields_are_not_keys(self):
        raw = b"\x1b[MqZt"
        self.assertEqual(self.frames([raw]), [raw])
        self.assertEqual(probe.decode_report(raw, 0)["kind"], "mouse")

    def test_utf8_offsets_and_maximum(self):
        for raw, x in [(b"\x1b[M\x20\x7f\x22", 95),
                       (b"\x1b[M\x20\xc2\x80\x22", 96),
                       (b"\x1b[M\x20\xc2\xa0\x22", 128),
                       (b"\x1b[M\x20\xdf\xbf\x22", 2015)]:
            self.assertEqual(probe.decode_report(raw, 1005)["x"], x)
        for raw in [b"\x1b[M\x20\xe0\xa0\x80\x22", b"\x1b[M\x20\xc2q\x22"]:
            self.assertEqual(self.frames([raw], 1005), [raw])
            with self.assertRaises((ValueError, UnicodeError)):
                probe.decode_report(raw, 1005)

    def test_invalid_codes_ranges_and_finals(self):
        for encoding, raw in [(1006, b"\x1b[<0;0;2M"),
                              (1006, b"\x1b[<66;3;2m"),
                              (1006, b"\x1b[<3;3;2M"),
                              (1006, b"\x1b[<3;3;2m"),
                              (1015, b"\x1b[32;3;2m"),
                              (1006, b"\x1b[<0;4294967296;2M"),
                              (0, b"\x1b[M\x20\x20\x22"),
                              (1006, b"\x1b[<255;3;2M")]:
            with self.assertRaises(ValueError):
                probe.decode_report(raw, encoding)
        for encoding in (1006, 1016):
            for code in (3, 31):
                for final in ("M", "m"):
                    with self.assertRaises(ValueError):
                        probe.decode_report(f"\x1b[<{code};3;2{final}".encode(), encoding)
            self.assertEqual(probe.decode_report(b"\x1b[<35;3;2M", encoding)["action"], "motion")

    def test_wheel_order_buttons_and_modifiers(self):
        frames = self.frames([b"\x1b[<93;3;2M\x1b[<94;3;2M"], 1006)
        decoded = [probe.decode_report(raw, 1006) for raw in frames]
        self.assertEqual([x["action"] for x in decoded], ["wheel-down", "wheel-left"])
        self.assertEqual([x["modifiers"] for x in decoded], [28, 28])

    def test_partial_input_can_expire_and_is_bounded(self):
        framer = probe.Framer()
        framer.pending.extend(b"\x1b[?1016$")
        self.assertIsNone(framer.pop(0))
        self.assertEqual(framer.expire(), b"\x1b[?1016$")
        framer.pending.extend(b"\x1b[" + b"1" * 257)
        self.assertIsNotNone(framer.pop(0))
        self.assertFalse(framer.pending)


class ProbeTests(unittest.TestCase):
    def setUp(self):
        self.console, self.log = FakeConsole(), io.StringIO()
        self.p = probe.Probe(self.console, self.log)
        self.p.flags = {1000, 1006}

    def records(self):
        return [json.loads(line) for line in self.log.getvalue().splitlines()]

    def confirm(self):
        if not self.p.pending_queries:
            self.p.query()
        for mode, status in list(self.p.pending_queries.items()):
            self.p.frame(f"\x1b[?{mode};{status}$y".encode())

    def test_queries_confirm_each_flag_including_unknown(self):
        self.confirm()
        self.assertTrue(self.p.confirmed)
        self.assertEqual(self.p.counts["query_pass"], 8)
        self.assertEqual(self.records()[-1]["expected"], 0)
        self.assertEqual(self.records()[-1]["kind"], "mode_reply")
        self.assertEqual(self.records()[-1]["decoded_kind"], "reply")

    def test_query_mismatch_and_timeout_never_confirm(self):
        self.p.query()
        self.p.frame(b"\x1b[?1006;2$y")
        self.assertEqual(self.records()[-1]["outcome"], "NEEDS_VERIFICATION")
        for mode, status in list(self.p.pending_queries.items()):
            self.p.frame(f"\x1b[?{mode};{status}$y".encode())
        self.assertFalse(self.p.confirmed)
        self.assertEqual(self.p.counts["query_mismatch"], 1)
        self.p.query()
        self.p.query_deadline = -1
        self.p.check_timeout()
        self.assertFalse(self.p.confirmed)
        self.assertEqual(self.p.counts["query_timeout"], 8)
        self.assertEqual(self.records()[-1]["outcome"], "NEEDS_VERIFICATION")

    def test_expired_replies_and_retries_cannot_confirm(self):
        self.p.query()
        replies = list(self.p.pending_queries.items())
        self.p.query_deadline = -1
        for mode, status in replies:
            self.p.frame(f"\x1b[?{mode};{status}$y".encode())
        self.assertFalse(self.p.confirmed)
        self.assertEqual(self.p.counts["query_pass"], 0)
        self.assertEqual(self.p.counts["query_timeout"], 8)
        self.confirm()
        self.assertFalse(self.p.confirmed)  # Replies have no request ID after a timeout.

    def test_transition_mouse_payload_is_quarantined_at_every_split(self):
        for flags, stream in [
            ({1000}, b"2\x1b[M\x20\xc2\x22\x1b[Mq\x23\x22"),
            ({1000, 1005}, b"1\x1b[M\x20\xc2\xa0q"),
        ]:
            for split in range(len(stream) + 1):
                with self.subTest(flags=flags, split=split):
                    self.setUp()
                    self.p.flags = flags.copy()
                    self.confirm()
                    framer = probe.Framer()
                    with patch.object(self.p, "key", wraps=self.p.key) as keys:
                        running = True
                        for chunk in (stream[:split], stream[split:]):
                            if not running:
                                break
                            framer.pending.extend(chunk)
                            running = self.p.consume(framer)
                        self.assertFalse(running)
                        self.assertEqual([call.args[0] for call in keys.call_args_list], [stream[:1]])
                    self.assertFalse(framer.pending)
                    self.assertEqual(self.records()[-1]["kind"], "framing_stopped")
                    self.assertEqual(self.records()[-1]["outcome"], "NEEDS_VERIFICATION")

    def test_invalid_mouse_frame_never_dispatches_tail_as_keys(self):
        self.confirm()
        framer = probe.Framer()
        framer.pending.extend(b"\x1b[<0;0;2MqZ")
        with patch.object(self.p, "key", wraps=self.p.key) as keys:
            self.assertFalse(self.p.consume(framer))
            keys.assert_not_called()
        self.assertFalse(framer.pending)
        self.assertEqual(self.records()[-1]["hex"], "71 5a")
        self.assertEqual(self.p.counts["syntax_fail"], 1)

    def test_mouse_pass_mismatch_and_unconfirmed_state(self):
        self.p.frame(b"\x1b[<0;3;2M")
        self.assertEqual(self.records()[-1]["outcome"], "NEEDS_VERIFICATION")
        self.confirm()
        self.p.frame(b"\x1b[<0;3;2M")
        self.assertEqual(self.records()[-1]["outcome"], "PASS")
        self.p.frame(b"\x1b[32;3;2M")
        self.assertEqual(self.records()[-1]["outcome"], "FAIL")
        self.assertEqual(self.p.counts["mouse_fail"], 1)
        self.p.expect_position = (3, 2)
        self.p.frame(b"\x1b[<0;4;2M")
        self.assertEqual(self.records()[-1]["outcome"], "FAIL")

    def test_invalid_frame_is_failure_only_in_confirmed_state(self):
        self.p.frame(b"\x1b[<0;0;2M")
        self.assertEqual(self.records()[-1]["outcome"], "NEEDS_VERIFICATION")
        self.assertEqual(self.p.counts["syntax_unverified"], 1)
        self.assertEqual(self.p.counts["syntax_fail"], 0)
        self.confirm()
        self.p.frame(b"\x1b[<0;0;2M")
        self.assertEqual(self.records()[-1]["outcome"], "FAIL")
        self.assertEqual(self.p.counts["syntax_fail"], 1)

    def test_tracking_filter_checks(self):
        self.confirm()
        self.p.frame(b"\x1b[<35;3;2M")
        self.assertEqual(self.records()[-1]["outcome"], "FAIL")
        self.p.flags = {1002, 1006}
        self.confirm()
        self.p.frame(b"\x1b[<35;3;2M")
        self.assertEqual(self.records()[-1]["outcome"], "FAIL")
        self.p.flags = {1003, 1016}
        self.confirm()
        self.p.frame(b"\x1b[<35;3;2M")
        self.assertEqual(self.records()[-1]["outcome"], "PASS")
        self.assertEqual(self.records()[-1]["coordinate_units"], "pixel")

    def test_encoding_priority_and_fallback(self):
        self.p.key(b"a")
        self.assertEqual(self.p.encoding, 1016)
        for expected in (1006, 1015, 1005, 0):
            self.confirm()
            self.p.key(b"f")
            self.assertEqual(self.p.encoding, expected)
        self.assertEqual(probe.mode_command([1016], False), b"\x1b[?1016l")

    def test_tracking_priority_and_fallback(self):
        self.p.key(b"b")
        self.assertEqual(probe.effective(self.p.flags, probe.TRACKING), 1003)
        for expected in (1002, 1000, 0):
            self.confirm()
            self.p.key(b"d")
            self.assertEqual(probe.effective(self.p.flags, probe.TRACKING), expected)

    def test_individual_encoding_and_tracking_cycle(self):
        for key, expected in [(b"1", 0), (b"2", 1005), (b"3", 1015), (b"4", 1006), (b"5", 1016)]:
            self.confirm()
            self.p.key(key)
            self.assertEqual(self.p.encoding, expected)
            self.assertEqual(self.p.flags.intersection(probe.ENCODINGS), {expected} if expected else set())
        for expected in (1002, 1003, 0, 1000):
            self.confirm()
            self.p.key(b"t")
            self.assertEqual(probe.effective(self.p.flags, probe.TRACKING), expected)

    def test_pending_query_blocks_commands_but_not_quit(self):
        self.p.query()
        before = self.p.flags.copy()
        self.p.key(b"Z")
        self.assertEqual(self.p.flags, before)
        self.assertFalse(self.p.key(b"q"))
        self.assertFalse(self.p.key(b"\x03"))
        self.confirm()
        self.p.key(b"Z")
        self.assertFalse(self.p.flags)
        self.assertIn(b"\x1bc", self.console.written)

    def test_main_logs_no_mouse_as_not_run_and_restores(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "probe.jsonl"
            with patch.object(probe, "Console", return_value=self.console), patch.object(probe, "revision", return_value={}), patch("builtins.print"), patch.object(probe.os, "get_terminal_size", return_value=probe.os.terminal_size((100, 30))):
                self.assertEqual(probe.main(["--log", str(path)]), 0)
            records = [json.loads(line) for line in path.read_text().splitlines()]
            summary = next(item for item in records if item["kind"] == "summary")
            self.assertEqual(summary["mouse_observation"], "NOT_RUN")
            self.assertEqual(summary["overall"], "NOT_AUTOMATICALLY_ACCEPTED")
            self.assertTrue(self.console.closed)
            self.assertEqual(self.console.written[-1], probe.mode_command(probe.MODES, False) + b"\x1b[?1049l")
            with patch("builtins.print"):
                self.assertEqual(probe.main(["--log", str(path)]), 1)

    def test_setup_failure_still_restores(self):
        with tempfile.TemporaryDirectory() as directory:
            self.console.start = lambda: (_ for _ in ()).throw(OSError("fixture setup failure"))
            with patch.object(probe, "Console", return_value=self.console), patch.object(probe, "revision", return_value={}), patch("builtins.print"):
                self.assertEqual(probe.main(["--log", str(Path(directory) / "log")]), 1)
            self.assertTrue(self.console.closed)

    def test_nonfinite_duration_rejected_before_native_io(self):
        with patch.object(probe, "Console") as console, patch("sys.stderr", io.StringIO()):
            for value in ("nan", "inf", "-1"):
                with self.assertRaises(SystemExit):
                    probe.main(["--duration", value])
            console.assert_not_called()


class CleanupTests(unittest.TestCase):
    def test_native_restoration_attempts_all_saved_properties(self):
        console = probe.Console.__new__(probe.Console)
        console.stop, console.reader = threading.Event(), None
        console.stdin, console.stdout = 123, 456
        console.old_in, console.old_out = probe.w.DWORD(7), probe.w.DWORD(3)
        console.old_cp = 437
        calls = []
        class Api:
            def SetConsoleCP(self, value):
                calls.append(("cp", value))
                return True
            def SetConsoleMode(self, handle, value):
                calls.append(("mode", handle, value))
                return True
        console.api = Api()
        console.close()
        self.assertEqual(calls, [("cp", 437), ("mode", 456, 3), ("mode", 123, 7)])
        self.assertTrue(console.stop.is_set())

    def test_native_restoration_failure_does_not_skip_other_properties(self):
        console = probe.Console.__new__(probe.Console)
        console.stop, console.reader = threading.Event(), None
        console.stdin, console.stdout = 123, 456
        console.old_in, console.old_out = probe.w.DWORD(7), probe.w.DWORD(3)
        console.old_cp = 437
        calls = []

        class Api:
            def SetConsoleCP(self, value):
                calls.append(("cp", value))
                return False

            def SetConsoleMode(self, handle, value):
                calls.append(("mode", handle, value))
                return True

        console.api = Api()
        with patch.object(probe.ctypes, "get_last_error", return_value=5, create=True):
            with self.assertRaisesRegex(OSError, "restoration errors"):
                console.close()
        self.assertEqual(calls, [("cp", 437), ("mode", 456, 3), ("mode", 123, 7)])


if __name__ == "__main__":
    unittest.main()
