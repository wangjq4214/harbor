#!/usr/bin/env python3
"""Interactive application-side Windows VT mouse probe (standard library only).

Run INSIDE Harbor, not through a pipe. Raw JSONL logs default to the OS temp
folder. This observes ReadFile bytes AFTER ConPTY/Console processing; it does
not tap Harbor's input pipe or prove physical event units/deltas.
"""
from __future__ import annotations

import argparse
import ctypes
import math
from ctypes import wintypes as w
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import queue
import re
import subprocess
import sys
import tempfile
import threading
import time

VERSION = "1.0"
TRACKING = (1000, 1002, 1003)
ENCODINGS = (1005, 1015, 1006, 1016)
MODES = TRACKING + ENCODINGS
QUERIES = MODES + (9,)
NAMES = {0: "legacy", 1005: "utf8", 1015: "urxvt", 1006: "sgr", 1016: "pixel"}
SGR = re.compile(rb"\x1b\[<(\d+);(\d+);(\d+)([Mm])")
URXVT = re.compile(rb"\x1b\[(\d+);(\d+);(\d+)M")
REPLY = re.compile(rb"\x1b\[\?(\d+);(\d+)\$y")


def mode_command(modes: tuple[int, ...] | list[int], enabled: bool) -> bytes:
    return ("\x1b[?" + ";".join(map(str, modes)) + ("h" if enabled else "l")).encode()


def effective(flags: set[int], priority: tuple[int, ...]) -> int:
    return next((mode for mode in reversed(priority) if mode in flags), 0)


def decode_report(raw: bytes, encoding: int) -> dict:
    """Decode without treating bytes embedded in mouse fields as menu keys."""
    if match := REPLY.fullmatch(raw):
        return {"kind": "reply", "mode": int(match[1]), "status": int(match[2])}
    if match := SGR.fullmatch(raw):
        code, x, y = map(int, match.groups()[:3])
        wire = "sgr"
        release = match[4] == b"m"
    elif match := URXVT.fullmatch(raw):
        offset, x, y = map(int, match.groups())
        code, wire, release = offset - 32, "urxvt", False
    elif raw.startswith(b"\x1b[M"):
        fields = raw[3:].decode("utf-8") if encoding == 1005 else raw[3:].decode("latin1")
        if len(fields) != 3:
            raise ValueError("legacy/UTF-8 report must have three fields")
        code, x, y = (ord(field) - 32 for field in fields)
        wire, release = ("utf8" if encoding == 1005 else "legacy"), False
    else:
        if raw.startswith(b"\x1b[<") or re.match(rb"\x1b\[\d+;\d+;", raw):
            raise ValueError("malformed decimal mouse report")
        return {"kind": "unknown"}
    base = code & ~28
    if x < 1 or y < 1 or base not in (*range(4), *range(32, 36), *range(64, 68)):
        raise ValueError("invalid coordinates or event code")
    if wire == "legacy" and max(x, y) > 223:
        raise ValueError("legacy coordinate exceeds 223")
    if wire == "utf8" and max(x, y) > 2015:
        raise ValueError("UTF-8 coordinate exceeds 2015")
    if max(x, y) > 0xFFFFFFFF:
        raise ValueError("decimal coordinate exceeds u32")
    if release and base not in range(4):
        raise ValueError("SGR lowercase release cannot be motion/wheel")
    if wire == "sgr" and base == 3:
        raise ValueError("SGR press/release must retain button identity (0/1/2)")
    action = ({64: "wheel-up", 65: "wheel-down", 66: "wheel-left", 67: "wheel-right"}.get(base)
              or ("release" if release or (base == 3 and wire != "sgr") else
                  "motion" if base & 32 else "press"))
    return {"kind": "mouse", "wire": wire, "code": code, "x": x, "y": y,
            "action": action, "modifiers": code & 28}


class Framer:
    """Bounded byte-stream framing: reads are not protocol/event boundaries."""
    def __init__(self):
        self.pending = bytearray()

    def pop(self, encoding: int) -> bytes | None:
        data = self.pending
        if not data:
            return None
        if data[0] != 27:
            size = 1
        elif len(data) < 2:
            return None
        elif data[1] != ord("["):
            size = 2
        elif len(data) < 3:
            return None
        elif data[2] == ord("M"):
            size = 3
            for _ in range(3):
                if len(data) <= size:
                    return None
                lead = data[size]
                width = (2 if 0xC0 <= lead <= 0xDF else 3 if 0xE0 <= lead <= 0xEF else 4 if 0xF0 <= lead <= 0xF7 else 1) if encoding == 1005 else 1
                if len(data) < size + width:
                    return None
                size += width
        else:
            size = next((i + 1 for i in range(2, len(data)) if 0x40 <= data[i] <= 0x7E), 0)
            if not size:
                if len(data) <= 256:
                    return None
                size = len(data)
        raw = bytes(data[:size])
        del data[:size]
        return raw

    def expire(self) -> bytes:
        raw = bytes(self.pending)
        self.pending.clear()
        return raw


class Console:
    """Native console byte I/O, explicit Win32 signatures (including 64-bit handles)."""
    def __init__(self):
        if os.name != "nt":
            raise RuntimeError("Windows only; run this script in a Harbor console")
        import msvcrt
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = {
            "GetConsoleMode": ([w.HANDLE, ctypes.POINTER(w.DWORD)], w.BOOL),
            "SetConsoleMode": ([w.HANDLE, w.DWORD], w.BOOL),
            "GetConsoleCP": ([], w.UINT), "SetConsoleCP": ([w.UINT], w.BOOL),
            "ReadFile": ([w.HANDLE, w.LPVOID, w.DWORD, ctypes.POINTER(w.DWORD), w.LPVOID], w.BOOL),
            "WriteFile": ([w.HANDLE, w.LPCVOID, w.DWORD, ctypes.POINTER(w.DWORD), w.LPVOID], w.BOOL),
            "OpenThread": ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
            "CancelSynchronousIo": ([w.HANDLE], w.BOOL),
            "CloseHandle": ([w.HANDLE], w.BOOL),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.api, name)
            function.argtypes, function.restype = args, result
        self.stdin = w.HANDLE(msvcrt.get_osfhandle(sys.stdin.fileno()))
        self.stdout = w.HANDLE(msvcrt.get_osfhandle(sys.stdout.fileno()))
        self.old_in, self.old_out = w.DWORD(), w.DWORD()
        self.check(self.api.GetConsoleMode(self.stdin, ctypes.byref(self.old_in)))
        self.check(self.api.GetConsoleMode(self.stdout, ctypes.byref(self.old_out)))
        self.old_cp = self.api.GetConsoleCP()
        self.stop = threading.Event()
        self.reader = None
        self.inbox = queue.Queue(maxsize=128)

    @staticmethod
    def check(ok):
        if not ok:
            raise ctypes.WinError(ctypes.get_last_error())

    def start(self):
        # Raw VT input; disable processed/line/echo input and Quick Edit.
        self.check(self.api.SetConsoleMode(self.stdin, (self.old_in.value & ~0x47) | 0x280))
        self.check(self.api.SetConsoleMode(self.stdout, self.old_out.value | 4))
        self.check(self.api.SetConsoleCP(65001))
        self.reader = threading.Thread(target=self.read_loop, daemon=True)
        self.reader.start()

    def write(self, data: bytes):
        done = w.DWORD()
        self.check(self.api.WriteFile(self.stdout, data, len(data), ctypes.byref(done), None))
        if done.value != len(data):
            raise OSError("short console write")

    def read_loop(self):
        buffer, done = ctypes.create_string_buffer(4096), w.DWORD()
        while not self.stop.is_set():
            ok = self.api.ReadFile(self.stdin, buffer, len(buffer), ctypes.byref(done), None)
            item = bytes(buffer.raw[:done.value]) if ok else OSError(ctypes.get_last_error(), "ReadFile failed")
            if self.stop.is_set():
                break
            while not self.stop.is_set():
                try:
                    self.inbox.put(item, timeout=0.1)
                    break
                except queue.Full:
                    pass
            if not ok or not done.value:
                break

    def close(self):
        self.stop.set()
        if self.reader and self.reader.native_id is not None:
            # Cancel the exact blocking reader before restoring shell console modes.
            handle = self.api.OpenThread(1, False, self.reader.native_id)
            if handle:
                try:
                    for _ in range(20):
                        if not self.reader.is_alive():
                            break
                        self.api.CancelSynchronousIo(handle)
                        self.reader.join(0.05)
                finally:
                    self.api.CloseHandle(handle)
        # Best effort for every saved property even if another restoration fails.
        failures = []
        for function, args in [(self.api.SetConsoleCP, (self.old_cp,)),
                               (self.api.SetConsoleMode, (self.stdout, self.old_out.value)),
                               (self.api.SetConsoleMode, (self.stdin, self.old_in.value))]:
            if not function(*args):
                failures.append(ctypes.get_last_error())
        if self.reader and self.reader.is_alive():
            failures.append("reader cancellation did not complete")
        if failures:
            raise OSError(f"console restoration errors: {failures}")


def revision() -> dict:
    root = Path(__file__).resolve().parents[1]
    def git(*args):
        try:
            return subprocess.check_output(["git", *args], cwd=root, stderr=subprocess.DEVNULL, timeout=2).decode("utf-8", errors="replace").strip()
        except (OSError, subprocess.SubprocessError):
            return "unavailable"
    return {"revision": git("rev-parse", "HEAD"), "dirty_paths": git("status", "--short")}


class Probe:
    def __init__(self, console, log, expect_position=None):
        self.console, self.log = console, log
        self.flags: set[int] = set()
        self.pending_queries: dict[int, int] = {}
        self.query_deadline = 0.0
        self.confirmed = False
        self.query_failed = False
        self.query_ambiguous = False  # DECRQM has no request identifier after a timeout/retry.
        self.expect_position = expect_position
        self.counts = {
            "mouse": 0, "mouse_pass": 0, "mouse_fail": 0, "mouse_unverified": 0,
            "syntax_fail": 0, "syntax_unverified": 0,
            "query_pass": 0, "query_mismatch": 0, "query_timeout": 0,
        }
        self.lines = ["No input observed yet. Physical mouse tests remain NOT RUN."]

    def record(self, event_type, **fields):
        if "kind" in fields:
            fields["decoded_kind"] = fields.pop("kind")
        self.log.write(json.dumps({"time": datetime.now(timezone.utc).isoformat(), "kind": event_type, **fields}, ensure_ascii=True) + "\n")
        self.log.flush()

    @property
    def encoding(self):
        return effective(self.flags, ENCODINGS)

    def send(self, raw, label):
        self.record("output", label=label, hex=raw.hex(" "), requested_flags=sorted(self.flags))
        self.console.write(raw)

    def query(self):
        if self.pending_queries:
            self.query_ambiguous = True
        self.confirmed, self.query_failed = False, False
        self.pending_queries = {mode: 0 if mode == 9 else 1 if mode in self.flags else 2 for mode in QUERIES}
        self.query_deadline = time.monotonic() + 2.0
        self.send(b"".join(f"\x1b[?{mode}$p".encode() for mode in QUERIES), "DECRQM")

    def check_timeout(self):
        if self.pending_queries and time.monotonic() >= self.query_deadline:
            self.record("query_timeout", outcome="NEEDS_VERIFICATION", missing_modes=sorted(self.pending_queries))
            self.counts["query_timeout"] += len(self.pending_queries)
            self.pending_queries.clear()
            self.confirmed = False
            self.query_ambiguous = True
            self.lines.append("Query timeout: state not confirmed; possible Console/ConPTY interception.")

    def key(self, raw):
        if raw in (b"q", b"\x03"):
            return False
        if self.pending_queries:
            self.lines.append("Wait for mode queries (q still exits).")
            return True
        if raw in (b"1", b"2", b"3", b"4", b"5"):
            selected = (0, 1005, 1015, 1006, 1016)[int(raw) - 1]
            self.flags.difference_update(ENCODINGS)
            if selected:
                self.flags.add(selected)
            self.send(mode_command(ENCODINGS, False) + (mode_command([selected], True) if selected else b""), f"encoding {NAMES[selected]}")
        elif raw == b"a":
            self.flags.update(ENCODINGS)
            self.send(mode_command(ENCODINGS, True), "enable all encodings")
        elif raw == b"f" and self.encoding:
            selected = self.encoding
            self.flags.remove(selected)
            self.send(mode_command([selected], False), "disable winning encoding")
        elif raw == b"t":
            selected = (0, 1000, 1002, 1003)[((0, 1000, 1002, 1003).index(effective(self.flags, TRACKING)) + 1) % 4]
            self.flags.difference_update(TRACKING)
            if selected:
                self.flags.add(selected)
            self.send(mode_command(TRACKING, False) + (mode_command([selected], True) if selected else b""), f"tracking {selected}")
        elif raw == b"b":
            self.flags.update(TRACKING)
            self.send(mode_command(TRACKING, True), "enable all tracking")
        elif raw == b"d" and (selected := effective(self.flags, TRACKING)):
            self.flags.remove(selected)
            self.send(mode_command([selected], False), "disable winning tracking")
        elif raw == b"Z":
            self.flags.clear()
            self.send(b"\x1bc", "RIS (user requested; clears terminal state)")
        elif raw != b"v":
            self.record("key", hex=raw.hex(" "), note="not a menu command")
            return True
        self.query()
        return True

    def frame(self, raw):
        self.check_timeout()  # Never let replies arriving after the deadline confirm a batch.
        try:
            parsed = decode_report(raw, self.encoding)
        except (ValueError, UnicodeError) as error:
            outcome = "FAIL" if self.confirmed else "NEEDS_VERIFICATION"
            self.counts["syntax_fail" if self.confirmed else "syntax_unverified"] += 1
            self.record("invalid_frame", hex=raw.hex(" "), error=str(error), outcome=outcome,
                        requested_encoding=NAMES[self.encoding], modes_confirmed=self.confirmed)
            self.lines.append(f"{outcome} invalid frame: {raw.hex(' ')}")
            return False
        if parsed["kind"] == "reply":
            mode = parsed["mode"]
            expected = self.pending_queries.pop(mode, None)
            outcome = "UNSOLICITED" if expected is None else "PASS" if parsed["status"] == expected else "NEEDS_VERIFICATION"
            if expected is not None:
                self.counts["query_pass" if outcome == "PASS" else "query_mismatch"] += 1
                self.query_failed |= outcome != "PASS"
                if not self.pending_queries:
                    self.confirmed = not self.query_failed and not self.query_ambiguous
            self.record("mode_reply", **parsed, expected=expected, outcome=outcome, hex=raw.hex(" "))
            self.lines.append(f"{outcome} ?{mode}: got {parsed['status']}, expected {expected}")
        elif parsed["kind"] == "mouse":
            self.counts["mouse"] += 1
            expected_wire = "sgr" if self.encoding in (1006, 1016) else NAMES[self.encoding]
            errors = []
            if parsed["wire"] != expected_wire:
                errors.append(f"expected {expected_wire} wire")
            tracking = effective(self.flags, TRACKING)
            if not tracking or (tracking == 1000 and parsed["action"] == "motion") or (tracking == 1002 and (parsed["code"] & ~28) == 35):
                errors.append("report conflicts with requested tracking filter")
            if self.expect_position and (parsed["x"], parsed["y"]) != tuple(self.expect_position):
                errors.append(f"expected position {tuple(self.expect_position)}")
            outcome = ("FAIL" if errors else "PASS") if self.confirmed else "NEEDS_VERIFICATION"
            self.counts[{"PASS": "mouse_pass", "FAIL": "mouse_fail", "NEEDS_VERIFICATION": "mouse_unverified"}[outcome]] += 1
            self.record("mouse_report", **parsed, hex=raw.hex(" "), outcome=outcome, errors=errors,
                        requested_encoding=NAMES[self.encoding], modes_confirmed=self.confirmed,
                        coordinate_units="pixel" if self.encoding == 1016 else "cell")
            self.lines.append(f"{outcome} {parsed['action']} code={parsed['code']} pos=({parsed['x']},{parsed['y']}) hex={raw.hex(' ')}")
        else:
            self.record("unknown_frame", hex=raw.hex(" "))
            self.lines.append(f"Unrecognized input: {raw.hex(' ')}")
        return True

    def consume(self, framer):
        """Never turn ambiguous/malformed mouse payload tails into menu commands."""
        while framer.pending:
            self.check_timeout()
            if not self.confirmed and framer.pending.startswith(b"\x1b[M"):
                self.record("framing_stopped", outcome="NEEDS_VERIFICATION",
                            reason="legacy/UTF-8 boundary ambiguous before mode confirmation",
                            hex=framer.expire().hex(" "), requested_flags=sorted(self.flags))
                return False
            raw = framer.pop(self.encoding)
            if raw is None:
                break
            if len(raw) == 1 and raw[0] != 27:
                if not self.key(raw):
                    return False
            elif not self.frame(raw):
                self.record("framing_stopped", outcome="NEEDS_VERIFICATION",
                            reason="invalid mouse frame; remaining input quarantined",
                            hex=framer.expire().hex(" "), requested_flags=sorted(self.flags))
                return False
        return True

    def draw(self):
        self.lines = self.lines[-6:]
        screen = [f"Harbor Windows mouse probe {VERSION} (q/Ctrl-C quits)",
                  "123456789" * 8,
                  "Click row 2 / column 3 for the documented (3,2) cell example.",
                  "1 legacy | 2 UTF8 | 3 urxvt | 4 SGR | 5 pixel | a all | f fallback",
                  "t tracking cycle off/1000/1002/1003 | b all tracking | d fallback",
                  "v query | uppercase Z RIS (CLEARS terminal state/history!)",
                  f"Requested: tracking={effective(self.flags, TRACKING)} encoding={NAMES[self.encoding]} flags={sorted(self.flags)}",
                  f"Modes confirmed={self.confirmed}. PASS means scoped byte/flag checks, NOT full acceptance.",
                  "Wheel increment units, native delta sign, DPI and local/capture behavior require human evidence.",
                  *self.lines]
        size = os.get_terminal_size(sys.stdout.fileno())
        # Fixed rows avoid output-induced scrolling while the pointer stays stationary.
        content = "\x1b[H" + "".join("\x1b[2K" + line[:max(1, size.columns - 1)] + "\r\n" for line in screen[:max(1, size.lines - 1)])
        self.console.write(content.encode("ascii", errors="replace"))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--encoding", choices=NAMES.values(), default="sgr")
    parser.add_argument("--tracking", choices=(0, *TRACKING), type=int, default=1000)
    parser.add_argument("--log", type=Path, help="new JSONL file (never overwrites)")
    parser.add_argument("--label", default="device/DPI/Harbor executable not recorded", help="device, DPI, build and scenario notes")
    parser.add_argument("--expect-position", nargs=2, type=int, metavar=("X", "Y"), help="optional exact position check on EVERY mouse report; use only stationary clicks/wheels")
    parser.add_argument("--duration", type=float, default=0, help="exit after this many seconds; 0 waits for q")
    args = parser.parse_args(argv)
    if not math.isfinite(args.duration) or args.duration < 0 or (args.expect_position and min(args.expect_position) < 1):
        parser.error("duration must be finite and nonnegative; expected coordinates must be positive")
    path = args.log or Path(tempfile.gettempdir()) / f"harbor-mouse-{datetime.now():%Y%m%d-%H%M%S}-{os.getpid()}.jsonl"
    console = None
    try:
        with path.open("x", encoding="utf-8") as log:
            console = Console()
            probe = Probe(console, log, args.expect_position)
            probe.record("environment", probe_version=VERSION, python=sys.version, os=platform.platform(), label=args.label,
                         api="ReadFile + ENABLE_VIRTUAL_TERMINAL_INPUT", input_codepage=65001,
                         transport="application-visible Console bytes; ConPTY backend/version must be recorded separately",
                         **revision())
            try:
                console.start()
                console.write(b"\x1b[?1049h\x1b[2J\x1b[H")
                selected = next(mode for mode, name in NAMES.items() if name == args.encoding)
                probe.flags = {mode for mode in (args.tracking, selected) if mode}
                probe.send(mode_command(MODES, False) + (mode_command(sorted(probe.flags), True) if probe.flags else b""), "initial modes")
                probe.query()
                framer, began, received_at, painted_at = Framer(), time.monotonic(), 0.0, 0.0
                running = True
                while running and (not args.duration or time.monotonic() - began < args.duration):
                    try:
                        data = console.inbox.get(timeout=0.02)
                    except queue.Empty:
                        data = None
                    if isinstance(data, BaseException):
                        raise data
                    if data == b"":
                        break
                    if data:
                        probe.record("input_chunk", hex=data.hex(" "), requested_flags=sorted(probe.flags))
                        framer.pending.extend(data)
                        received_at = time.monotonic()
                    running = probe.consume(framer)
                    if framer.pending and time.monotonic() - received_at > 0.5:
                        probe.record("incomplete_input", outcome="NEEDS_VERIFICATION", hex=framer.expire().hex(" "))
                    probe.check_timeout()
                    if time.monotonic() - painted_at > 0.1:
                        probe.draw()
                        painted_at = time.monotonic()
                if framer.pending:
                    probe.record("incomplete_input", hex=framer.expire().hex(" "), outcome="NEEDS_VERIFICATION")
                if probe.pending_queries:
                    probe.record("queries_not_observed", outcome="NOT_RUN", modes=sorted(probe.pending_queries))
                probe.record("summary", **probe.counts, overall="NOT_AUTOMATICALLY_ACCEPTED",
                             mouse_observation="OBSERVED" if probe.counts["mouse"] else "NOT_RUN",
                             note="Manual scenarios/device/DPI/nvim evidence remain separate; PASS is only a scoped check.")
            finally:
                try:
                    # Exit alternate screen even if initial setup or the read loop failed.
                    console.write(mode_command(MODES, False) + b"\x1b[?1049l")
                finally:
                    console.close()
        print(f"Mouse probe log: {path}")
        return 0
    except (OSError, RuntimeError, KeyboardInterrupt) as error:
        print(f"Mouse probe stopped: {error}\nLog (if created): {path}\nRun in Harbor without stdin/stdout redirection. Inspect the log; this is not an acceptance pass.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
