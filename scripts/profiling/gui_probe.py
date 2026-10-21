"""Synthetic interactive child for Computer Use observations; no shell execution.

Commands: latin, colored, unicode, dirty, sustain, grid, alt, primary, quit.
Producer timings are NOT terminal input/output-to-present latency.
"""

import json
import os
import shutil
import sys
import threading
import time

from workload import rows

LOCK = threading.Lock()
JOB = None


def write(text):
    with LOCK:
        sys.stdout.write(text)
        sys.stdout.flush()


def emit(mode, count=20_000, delay=0):
    start = time.perf_counter_ns()
    total = 0
    write(f"BEGIN mode={mode} rows={count} grid={shutil.get_terminal_size()}\n")
    for row in rows(mode, count, 80):
        with LOCK:
            sys.stdout.buffer.write(row)
            sys.stdout.buffer.flush()
        total += len(row)
        if delay:
            time.sleep(delay)
    elapsed = time.perf_counter_ns() - start
    write(f"\nDONE mode={mode} rows={count} bytes={total} producer_ms={elapsed / 1e6:.3f}\nREADY\n")


def main():
    global JOB
    # Deterministic label instead of the user's shell title, path or profile.
    write("\x1b]0;Harbor synthetic probe\x07\x1b[?25l")
    write("HARBOR GUI PROBE schema=1\n")
    write(f"Python={sys.version.split()[0]} child_pid={os.getpid()}\n")
    write("Commands: latin colored unicode dirty sustain grid alt primary quit\nREADY\n")
    while True:
        try:
            command = input().strip()
        except EOFError:
            break
        if command == "quit":
            if JOB is not None and JOB.is_alive():
                write("BUSY: wait for synthetic output to finish\n")
                continue
            break
        if command == "grid":
            write(json.dumps({"grid": list(shutil.get_terminal_size()), "child_pid": os.getpid()}) + "\nREADY\n")
        elif command == "alt":
            write("\x1b[?1049h\x1b[HAlternate synthetic buffer\ncombining=e\u0301 VS=\u2764\ufe0f ZWJ=👩\u200d💻 CJK=界 emoji=😀\nREADY\n")
        elif command == "primary":
            write("\x1b[?1049l\nPRIMARY RESTORED\nREADY\n")
        elif command == "sustain":
            if JOB is not None and JOB.is_alive():
                write("BUSY\n")
            else:
                JOB = threading.Thread(target=emit, args=("colored", 6000, 0.005), daemon=True)
                JOB.start()
        elif command in ("latin", "colored", "unicode", "dirty"):
            if JOB is not None and JOB.is_alive():
                write("BUSY\n")
            else:
                emit(command)
        elif command:
            write("Unknown synthetic command (no shell execution)\nREADY\n")
    write("\x1b[?25h")


if __name__ == "__main__":
    main()
