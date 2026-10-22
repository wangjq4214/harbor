"""Bounded synthetic UTF-8/VT output; producer time is not terminal throughput."""

import argparse
import hashlib
import json
import sys
import time

MODES = ("latin", "colored", "unicode", "dirty")


def bounded_int(low, high):
    def parse(value):
        number = int(value)
        if not low <= number <= high:
            raise argparse.ArgumentTypeError(f"expected {low}..{high}")
        return number

    return parse


def rows(mode, count, columns):
    """Yield complete rows, never split a Unicode control or emit personal data."""
    if mode not in MODES or not 1 <= count <= 200_000 or not 16 <= columns <= 500:
        raise ValueError("unsupported mode or out-of-bounds count/columns")
    for index in range(count):
        label = f"{index:06d} "
        if mode == "unicode":
            # Ordinary combining, VS16, ZWJ, CJK and emoji; no exceptional growth.
            text = label + "e\u0301 \u2764\ufe0f 👩\u200d💻 界 😀 "
        else:
            text = label + "x" * (columns - len(label) - 1)
        if mode == "colored":
            text = f"\x1b[38;2;{index % 256};80;160m{text}\x1b[0m"
        elif mode == "dirty":
            text = f"\x1b[{index % 24 + 1};1H{text}"
        yield (text + ("\r\n" if mode != "dirty" else "\r")).encode("utf-8")


def describe(mode, count, columns):
    digest = hashlib.sha256()
    total = 0
    for row in rows(mode, count, columns):
        digest.update(row)
        total += len(row)
    return {
        "schema": 1,
        "mode": mode,
        "rows": count,
        "columns": columns,
        "dirty_grid_rows": 24 if mode == "dirty" else None,
        "bytes": total,
        "sha256": digest.hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=MODES, required=True)
    parser.add_argument("--rows", type=bounded_int(1, 200_000), default=20_000)
    parser.add_argument("--columns", type=bounded_int(16, 500), default=80)
    parser.add_argument("--delay-ms", type=bounded_int(0, 1000), default=0)
    parser.add_argument("--describe", action="store_true", help="hash/size only; no terminal output")
    args = parser.parse_args()
    if args.describe:
        result = describe(args.mode, args.rows, args.columns)
        result["delay_ms"] = args.delay_ms
        print(json.dumps(result, sort_keys=True))
        return
    for row in rows(args.mode, args.rows, args.columns):
        sys.stdout.buffer.write(row)
        if args.delay_ms:
            sys.stdout.buffer.flush()
            time.sleep(args.delay_ms / 1000)
    sys.stdout.buffer.flush()


if __name__ == "__main__":
    main()
