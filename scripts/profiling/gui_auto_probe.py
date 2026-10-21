"""Self-driving synthetic probe for bounded Computer Use measurements.

Harbor's terminal input path maps WM_KEYDOWN logical keys, so WM_CHAR-style
automation does not reach this child. Instead of typed commands, this probe
executes a bounded JSON plan so every phase is deterministic and reproducible.

Usage: python -u gui_auto_probe.py PLAN.json

Steps: sleep_ms, grid, banner, emit {mode,rows,columns,delay_ms},
oversize, alt, primary, done. No shell execution, no environment reads.
Producer timings are not terminal input/output-to-present latency.
"""

import json
import os
import shutil
import sys
import time

from workload import rows


def now():
    return {
        "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "mono_ms": round(time.perf_counter_ns() / 1e6, 3),
    }


def write(text):
    sys.stdout.write(text)
    sys.stdout.flush()


def event(kind, **fields):
    payload = {"event": kind, **now(), **fields}
    write(json.dumps(payload, sort_keys=True) + "\n")


def emit(mode, count, columns, delay_ms):
    event("emit_begin", mode=mode, rows=count, columns=columns, delay_ms=delay_ms)
    start = time.perf_counter_ns()
    total = 0
    for row in rows(mode, count, columns):
        sys.stdout.buffer.write(row)
        sys.stdout.buffer.flush()
        total += len(row)
        if delay_ms:
            time.sleep(delay_ms / 1000)
    elapsed = time.perf_counter_ns() - start
    event(
        "emit_end",
        mode=mode,
        rows=count,
        bytes=total,
        producer_ms=round(elapsed / 1e6, 3),
    )
    write(f"DONE mode={mode} rows={count} bytes={total} producer_ms={elapsed / 1e6:.3f}\n")


def oversize():
    combining = "e" + "\u0301" * 2_000
    zwj = "\U0001f469\u200d\U0001f4bb" + "\u200d\U0001f469" * 2_000
    event("oversize_begin", combining_bytes=len(combining.encode()), zwj_bytes=len(zwj.encode()))
    write("BEGIN oversize\n")
    write(f"combining_source_bytes={len(combining.encode())}\n")
    sys.stdout.buffer.write((combining + " MARKER-COMBINING-END\r\n").encode())
    sys.stdout.buffer.flush()
    write(f"zwj_source_bytes={len(zwj.encode())}\n")
    sys.stdout.buffer.write((zwj + " MARKER-ZWJ-END\r\n").encode())
    sys.stdout.buffer.flush()
    write("DONE oversize\n")
    event("oversize_end")


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: gui_auto_probe.py PLAN.json")
    with open(sys.argv[1], encoding="utf-8") as handle:
        plan = json.load(handle)
    write("\x1b]0;Harbor synthetic probe\x07\x1b[?25l")
    write("HARBOR AUTO PROBE schema=1\n")
    write(f"Python={sys.version.split()[0]} child_pid={os.getpid()}\n")
    event("start", plan=plan.get("name"), script_sha=os.environ.get("HARBOR_PLAN_SHA"))
    for step in plan["steps"]:
        if "sleep_ms" in step:
            event("sleep_begin", ms=step["sleep_ms"])
            time.sleep(step["sleep_ms"] / 1000)
            event("sleep_end")
        elif step.get("grid"):
            size = shutil.get_terminal_size()
            event("grid", columns=size.columns, lines=size.lines, child_pid=os.getpid())
            write(f"GRID {size.columns}x{size.lines}\n")
        elif "banner" in step:
            event("banner", text=step["banner"])
            write(f"== {step['banner']} ==\n")
        elif "emit" in step:
            spec = step["emit"]
            emit(
                spec.get("mode", "latin"),
                int(spec.get("rows", 20_000)),
                int(spec.get("columns", 80)),
                int(spec.get("delay_ms", 0)),
            )
        elif step.get("oversize"):
            oversize()
        elif step.get("alt"):
            event("alt_enter")
            write("\x1b[?1049h\x1b[HAlternate synthetic buffer\ncombining=e\u0301 VS=\u2764\ufe0f ZWJ=👩\u200d💻 CJK=界 emoji=😀\n")
        elif step.get("primary"):
            event("alt_exit")
            write("\x1b[?1049lPRIMARY RESTORED\n")
        elif step.get("done"):
            event("plan_done")
            write("PLAN-DONE\n")
        else:
            raise SystemExit(f"unsupported step: {step}")
    event("finish")
    write("PROBE-IDLE\n")
    # Keep the session alive so the terminal can be inspected after the plan.
    while True:
        try:
            time.sleep(3600)
        except KeyboardInterrupt:
            break


if __name__ == "__main__":
    main()
