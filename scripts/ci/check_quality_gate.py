"""Fail the stable quality status unless all required workflow lanes succeeded."""

import json
import os
from pathlib import Path
import sys


REQUIRED_JOBS = ("quick", "rust", "package")


def evaluate(raw: str) -> tuple[bool, str]:
    """Consume Actions' needs JSON, never treating missing or skipped work as a pass."""
    needs = json.loads(raw)
    if not isinstance(needs, dict):
        raise ValueError("needs must be a JSON object")
    results = {}
    for job in REQUIRED_JOBS:
        details = needs.get(job)
        result = details.get("result") if isinstance(details, dict) else None
        # Only trusted Actions result strings enter the Markdown/log output.
        results[job] = result if result in (
            "success", "failure", "cancelled", "skipped"
        ) else "missing or invalid"
    summary = "## Windows quality gates\n\n| Job | Result |\n| --- | --- |\n"
    summary += "".join(f"| {job} | {result} |\n" for job, result in results.items())
    return all(result == "success" for result in results.values()), summary


def main() -> int:
    try:
        passed, summary = evaluate(os.environ.get("NEEDS_JSON", ""))
    except ValueError as error:
        print(f"Invalid quality gate input: {error}", file=sys.stderr)
        return 1
    print(summary, end="")
    if path := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(path).open("a", encoding="utf-8") as output:
            output.write(summary)
    if not passed:
        print("Every required lane must succeed; failed, cancelled or skipped work blocks quality.",
              file=sys.stderr)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
