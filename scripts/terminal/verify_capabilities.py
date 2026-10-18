#!/usr/bin/env python3
"""Bounded POSIX-side negotiation probe for WSL/SSH, inside a disposable Harbor session.

Uses a real tty; never run through a pipe. The Windows capability_probe_host example
can supply structured keys when --keys is selected. No SSH connection is opened by
this script. Reports contain controlled query bytes and whitelisted metadata only.
"""
from __future__ import annotations

import argparse
import json
import os
from datetime import datetime, timezone
from pathlib import Path
import platform
import select
import subprocess
import sys
import tempfile
import time

ESC = b"\x1b"
ST = ESC + b"\\"
KEYS = (("up", "kcuu1", b"A"), ("down", "kcud1", b"B"),
        ("right", "kcuf1", b"C"), ("left", "kcub1", b"D"),
        ("home", "khome", b"H"), ("end", "kend", b"F"))


def complete(data: bytes) -> bool:
    if data.startswith(ESC + b"P"):
        return data.endswith(ST)
    if data.startswith(ESC + b"["):
        return any(0x40 <= byte <= 0x7E for byte in data[2:])
    if data.startswith(ESC + b"O"):
        return len(data) >= 3
    return bool(data) and data != ESC


def tcap(names: list[str]) -> bytes:
    return ESC + b"P+q" + b";".join(name.encode().hex().encode() for name in names) + ST


def read_reply(fd: int, timeout: float) -> bytes:
    deadline = time.monotonic() + timeout
    data = bytearray()
    while time.monotonic() < deadline:
        wait = max(0, deadline - time.monotonic())
        if not select.select([fd], [], [], wait)[0]:
            break
        chunk = os.read(fd, 4096 - len(data))
        if not chunk:
            break
        data.extend(chunk)
        if complete(data) or len(data) == 4096:
            break
    return bytes(data)


def command_version(*args: str) -> str:
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=3, check=False)
        return result.stdout.splitlines()[0] if result.stdout else "unavailable"
    except (OSError, subprocess.SubprocessError):
        return "unavailable"


def tmux_context(layer: str) -> dict | None:
    if layer != "tmux":
        return None
    pane = os.environ.get("TMUX_PANE", "")
    if not os.environ.get("TMUX") or not pane:
        raise RuntimeError("--layer tmux requires an active tmux pane, not merely an installed tmux")
    def display(format_string: str) -> str:
        return command_version("tmux", "display-message", "-p", "-t", pane, format_string)
    if display("#{pane_id}") != pane:
        raise RuntimeError("cannot verify the active tmux pane")
    return {"pane": pane, "outer_term": display("#{client_termname}"),
            "client_features": display("#{client_termfeatures}"),
            "default_terminal": command_version("tmux", "show-options", "-gv", "default-terminal")}


def vim_workload() -> list[dict]:
    """Exercise Vim's real terminfo key decoder and movement, not forced escape mappings."""
    with tempfile.TemporaryDirectory(prefix="harbor-capability-vim-") as directory:
        root = Path(directory)
        result = root / "result.json"
        script = root / "probe.vim"
        script.write_text(r'''
set nomore noswapfile
set shortmess+=I
call setline(1, ['first line', 'second row', 'third line'])
let results = []
for [name, expected_key, row, col] in [
      \ ['up', "\<Up>", 1, 4], ['down', "\<Down>", 3, 4],
      \ ['right', "\<Right>", 2, 5], ['left', "\<Left>", 2, 3],
      \ ['home', "\<Home>", 2, 1], ['end', "\<End>", 2, 10]]
  call cursor(2, 4)
  redraw!
  echo 'HARBOR_VIM_READY:application:' . name
  let started = reltime()
  let key = 0
  while key is# 0 && reltimefloat(reltime(started)) < 5.0
    let key = getchar(0)
    if key is# 0 | sleep 10m | endif
  endwhile
  if type(key) == v:t_number | let key = nr2char(key) | endif
  execute 'normal! ' . key
  call add(results, {'name': 'vim ' . name,
        \ 'status': key ==# expected_key && line('.') == row && col('.') == col ? 'PASS' : 'FAIL',
        \ 'expected_key': keytrans(expected_key), 'observed_key': keytrans(key),
        \ 'expected_position': [row, col], 'observed_position': [line('.'), col('.')]})
endfor
call writefile([json_encode(results)], $HARBOR_VIM_RESULT)
qa!
''', encoding="utf-8")
        environment = dict(os.environ, HARBOR_VIM_RESULT=str(result))
        try:
            process = subprocess.run(["vim", "-Nu", "NONE", "-n", "-i", "NONE", "-S", str(script)],
                                     env=environment, timeout=45, check=False)
            if process.returncode != 0 or not result.is_file():
                raise RuntimeError(f"Vim exited {process.returncode} without a successful result")
            return json.loads(result.read_text(encoding="utf-8"))
        except (OSError, subprocess.SubprocessError, ValueError, RuntimeError) as error:
            return [{"name": "vim workload", "status": "FAIL", "error": str(error)}]


def run(args: argparse.Namespace) -> dict:
    import curses
    import termios
    import tty

    if not sys.stdin.isatty() or not sys.stdout.isatty():
        raise RuntimeError("requires an interactive POSIX tty (WSL or SSH), not a pipe")
    fd = sys.stdin.fileno()
    term = os.environ.get("TERM", "")
    curses.setupterm(term=term, fd=sys.stdout.fileno())
    capabilities = {name: curses.tigetstr(name) for name in ("smkx", "rmkx", *(key[1] for key in KEYS))}
    report = {
        "layer": args.layer, "path": args.path, "platform": platform.platform(),
        "tmux": tmux_context(args.layer),
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "source": command_version("git", "-C", str(Path(__file__).resolve().parents[2]), "describe", "--always", "--dirty"),
        "environment": {key: os.environ.get(key) for key in ("TERM", "LANG", "LC_ALL", "LC_CTYPE")},
        "tmux_version": command_version("tmux", "-V"),
        "vim_version": command_version("vim", "--version"),
        "nvim_version": command_version("nvim", "--version"),
        "terminfo": {key: value.hex() if value is not None else None for key, value in capabilities.items()},
        "terminfo_colors": curses.tigetnum("colors"), "cases": [],
    }
    cases = report["cases"]

    def write(data: bytes):
        sys.stdout.buffer.write(data)
        sys.stdout.buffer.flush()

    def record(name: str, expected: bytes | None, observed: bytes, request: bytes = b"", note: str = ""):
        status = "NOT RUN" if expected is None else "PASS" if observed == expected else "FAIL"
        cases.append({"name": name, "status": status, "request_hex": request.hex(),
                      "expected_hex": expected.hex() if expected is not None else None,
                      "observed_hex": observed.hex(), "note": note})

    def query(name: str, request: bytes, expected: bytes, outer_only: bool = False):
        write(request)
        observed = read_reply(fd, args.timeout)
        if outer_only and args.layer == "tmux":
            record(name, None, observed, request,
                   "Captured only: tmux owns/filters this reply; Harbor's exact identity is not expected here.")
        else:
            record(name, expected, observed, request)

    old = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        # Controlled reset of only the state exercised here; use a disposable tab.
        write(ESC + b"[0m" + ESC + b"[?6l" + ESC + b"[?69l" + ESC + b"[r" + ESC + b"[H")
        query("DA1", ESC + b"[c", ESC + b"[?62;6;17;22;28c", True)
        query("DA2", ESC + b"[>c", ESC + b"[>1;1;0c", True)
        query("DSR", ESC + b"[5n", ESC + b"[0n")
        write(ESC + b"[3;7H")
        query("CPR", ESC + b"[6n", ESC + b"[3;7R")
        query("private CPR", ESC + b"[?6n", ESC + b"[?3;7R", True)
        write(ESC + b"[?1l")
        query("DECRQM reset", ESC + b"[?1$p", ESC + b"[?1;2$y", True)
        write(ESC + b"[?1h")
        query("DECRQM set", ESC + b"[?1$p", ESC + b"[?1;1$y", True)
        query("DECRQSS SGR", ESC + b"P$qm" + ST, ESC + b"P1$r0m" + ST, True)
        for name, value in [("TN", b"xterm-256color"), ("Co", b"256"), ("colors", b"256"),
                            ("U8", b"1"), ("u8", None), ("RGB", b"8/8/8"), ("Su", None)]:
            payload = name.encode().hex().upper().encode()
            if value is not None:
                payload += b"=" + value.hex().upper().encode()
            query("XTGETTCAP " + name, tcap([name]), ESC + b"P1+r" + payload + ST, True)
        query("XTGETTCAP unknown", tcap(["harbor-no-such-capability"]), ESC + b"P0+r" + ST, True)
        for name, request in [("tertiary DA", ESC + b"[=c"), ("XTVERSION", ESC + b"[>q"),
                              ("Kitty keyboard", ESC + b"[?u")]:
            query(name + " intentionally silent", request, b"", True)
        # Positive recovery after unknown/negative requests is checked by another DSR.
        query("query recovery", ESC + b"[5n", ESC + b"[0n")
        if args.keys:
            modes = ("normal", "application") if args.layer == "harbor" else ("application",)
            for mode in modes:
                write(capabilities["smkx" if mode == "application" else "rmkx"] or b"")
                for name, cap, suffix in KEYS:
                    write(f"\r\nHARBOR_KEY_READY:{mode}:{name}\r\n".encode())
                    observed = read_reply(fd, args.timeout)
                    expected = capabilities[cap] if mode == "application" else ESC + b"[" + suffix
                    record(f"key {mode} {name}", expected, observed,
                           note="Terminfo application-mode bytes" if mode == "application" else "xterm normal-mode bytes")
        else:
            record("physical/structured key input", None, b"", note="Use --keys and press each named key, or run capability_probe_host.")
        record("TERM identity", b"xterm-256color" if args.layer == "harbor" else None, term.encode(),
               note="tmux-inner TERM describes tmux, not Harbor" if args.layer == "tmux" else "Observed guest/remote environment")
        record("terminfo color count", b"256" if args.layer == "harbor" else None, str(report["terminfo_colors"]).encode())
        if args.vim:
            cases.extend(vim_workload())
        else:
            record("Vim workload", None, b"", note="Use --vim to exercise the real editor decoder and cursor movement.")
        record("Neovim workload", None, b"", note="Not exercised by this probe; Vim coverage does not establish Neovim behavior.")
    finally:
        try:
            write(capabilities["rmkx"] or ESC + b"[?1l" + ESC + b">")
            write(ESC + b"[0m\r\n")
        finally:
            termios.tcsetattr(fd, termios.TCSANOW, old)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--layer", choices=("harbor", "tmux"), default="harbor")
    parser.add_argument("--path", required=True, help="Named complete transport chain; no secrets")
    parser.add_argument("--output", type=Path, required=True, help="JSON report under target/ or an explicit output directory")
    parser.add_argument("--keys", action="store_true", help="Read named keys from a person or capability_probe_host")
    parser.add_argument("--vim", action="store_true", help="Run clean Vim decoder/movement checks; requires capability_probe_host")
    parser.add_argument("--timeout", type=float, default=2.0)
    args = parser.parse_args()
    if not 0 < args.timeout <= 5:
        parser.error("--timeout must be in (0, 5] seconds")
    try:
        report = run(args)
    except Exception as error:
        report = {"path": args.path, "cases": [{"name": "probe infrastructure", "status": "FAIL", "error": str(error)}]}
    status = "FAIL" if any(case["status"] == "FAIL" for case in report["cases"]) else "PASS"
    report["status"] = status
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"\r\nHARBOR_PROBE_DONE:{status}", flush=True)
    return 1 if status == "FAIL" else 0


if __name__ == "__main__":
    sys.exit(main())
