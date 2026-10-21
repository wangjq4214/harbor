"""Bounded original/control producer for an isolated native Harbor diagnosis."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import sys
import time

from unicode_diagnosis_input import generate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gate", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    sys.stdout.write("\x1b]0;Harbor Unicode diagnosis\x07\x1b[?25lWaiting for isolated fixture gate\n")
    sys.stdout.flush()
    deadline = time.monotonic() + 300
    while not args.gate.exists():
        if time.monotonic() > deadline:
            raise RuntimeError("diagnostic fixture gate not supplied within 300 seconds")
        time.sleep(0.05)
    results = []
    for round_id in range(3):
        for case in ("original", "no_vs16", "wide_vs16"):
            payload = list(generate(case, 2000))
            size = shutil.get_terminal_size()
            sys.stdout.write(f"\nBEGIN {case} round={round_id} grid={size.columns}x{size.lines}\n")
            sys.stdout.flush()
            start_utc = datetime.now(timezone.utc).isoformat()
            start = time.perf_counter_ns()
            for row in payload:
                sys.stdout.buffer.write(row)
                sys.stdout.buffer.flush()
            producer_ns = time.perf_counter_ns() - start
            results.append({
                "case": case, "round": round_id, "rows": 2000,
                "grid": [size.columns, size.lines],
                "input_bytes": sum(map(len, payload)),
                "input_sha256": hashlib.sha256(b"".join(payload)).hexdigest(),
                "start_utc": start_utc, "end_utc": datetime.now(timezone.utc).isoformat(),
                "producer_ns": producer_ns,
            })
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
            sys.stdout.write(f"\nDONE {case} producer_ms={producer_ns / 1e6:.3f}\n")
            sys.stdout.flush()
            # Same dwell for each case; does not certify the final display time.
            time.sleep(3)
    sys.stdout.write("\x1b[2J\x1b[HGUI DIAGNOSIS PRODUCER SUMMARY (NOT PRESENT LATENCY)\n")
    for result in results:
        sys.stdout.write(f"{result['case']} round={result['round']} producer_ms={result['producer_ns'] / 1e6:.3f}\n")
    sys.stdout.write("DIAGNOSIS_DONE\n")
    sys.stdout.flush()
    # Hold the owned window for capture; GUI close terminates this child normally.
    time.sleep(120)


if __name__ == "__main__":
    main()
