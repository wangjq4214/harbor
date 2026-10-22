"""Finite reduction of the measured Unicode burst; emits synthetic UTF-8 only."""

import argparse
import sys

from workload import bounded_int, rows

CASES = ("original", "no_vs16", "wide_vs16", "heart_vs16", "combining", "zwj", "cjk", "emoji")


def generate(case, count):
    if case not in CASES or not 1 <= count <= 20_000:
        raise ValueError("unsupported case or count")
    for index, row in enumerate(rows("unicode", count, 80)):
        if case == "original":
            yield row
        elif case == "no_vs16":
            yield row.replace("\ufe0f".encode(), b"")
        elif case == "wide_vs16":
            yield row.replace("\u2764".encode(), "😀".encode())
        else:
            unit = {
                "heart_vs16": "\u2764\ufe0f", "combining": "e\u0301",
                "zwj": "👩\u200d💻", "cjk": "界", "emoji": "😀",
            }[case]
            yield f"{index:06d} {unit}\r\n".encode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, choices=CASES)
    parser.add_argument("--rows", type=bounded_int(1, 20_000), default=2000)
    args = parser.parse_args()
    for row in generate(args.case, args.rows):
        sys.stdout.buffer.write(row)
    sys.stdout.buffer.flush()


if __name__ == "__main__":
    main()
