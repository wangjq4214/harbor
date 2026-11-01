"""Measure identical PR/base workloads and render an advisory report (never a merge gate)."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys


PREFIX = "HARBOR_PERF "
SCHEMA = 1
ROUNDS = 4
# IDs, descriptions and units are trusted presentation data, not benchmark stdout.
CORE = {
    "ingest_ascii": "ASCII output / 2000 rows",
    "ingest_ansi": "ANSI output / 2000 rows",
    "ingest_unicode": "Unicode output / 2000 rows",
    **{f"update_{mode}_{sessions}": f"Update {mode} / {sessions} sessions (per update)"
       for sessions in (1, 8) for mode in ("clean", "local")},
    **{f"resize_{kind}_{cols}": f"Primary resize {kind} / 160 → {cols} cols"
       for kind in ("ascii", "unicode") for cols in (80, 200)},
    "resize_alternate": "Alternate resize / 24×160 → 40×80",
}
PREPARE = {
    f"prepare_{fixture}_{mode}": f"WARP text prepare / {fixture} / {mode}"
    for fixture in ("ordinary", "emoji") for mode in ("clean", "local", "churn", "full")
}
STAGES = {"core": CORE, "prepare": PREPARE}
CASES = CORE | PREPARE
ERRORS = {"build failed", "measurement failed", "invalid measurement", "not measured"}


def positive(value):
    return type(value) in (int, float) and 0 < value <= 1e15 and math.isfinite(value)


def parse_output(output, stage):
    """Reject empty, duplicate, missing, unknown or nonfinite measurements."""
    expected = STAGES[stage]
    count = 101 if stage == "prepare" else 11
    rows = {}
    for line in output.splitlines():
        if not line.startswith(PREFIX):
            continue
        row = json.loads(line[len(PREFIX):])
        if not isinstance(row, dict) or set(row) != {"case", "samples_ns"}:
            raise ValueError("invalid measurement row")
        case, samples = row["case"], row["samples_ns"]
        if not isinstance(case, str) or case not in expected or case in rows:
            raise ValueError("unknown or duplicate case")
        if not isinstance(samples, list) or len(samples) != count or not all(map(positive, samples)):
            raise ValueError("invalid samples")
        rows[case] = samples
    if set(rows) != set(expected):
        raise ValueError("incomplete measurement")
    return rows


def validate_report(report):
    """Validate the bounded, untrusted transport before trusted comment rendering."""
    if not isinstance(report, dict) or type(report.get("schema")) is not int or report["schema"] != SCHEMA:
        raise ValueError("invalid report schema")
    for key in ("pr", "run_id", "run_number", "attempt"):
        if type(report.get(key)) is not int or report[key] <= 0:
            raise ValueError(f"invalid {key}")
    for key in ("base_sha", "head_sha", "merge_sha", "probe_sha256"):
        size = 64 if key == "probe_sha256" else 40
        value = report.get(key)
        if not isinstance(value, str) or not re.fullmatch(f"[0-9a-f]{{{size}}}", value):
            raise ValueError(f"invalid {key}")
    if type(report.get("rounds")) is not int or not 2 <= report["rounds"] <= 8:
        raise ValueError("invalid rounds")
    environment = report.get("environment")
    if not isinstance(environment, dict) or set(environment) != {"os", "cpu", "rustc"}:
        raise ValueError("invalid environment")
    if any(not isinstance(value, str) or not 1 <= len(value) <= 300
           for value in environment.values()):
        raise ValueError("invalid environment text")
    results = report.get("results")
    if not isinstance(results, dict) or set(results) != set(CASES):
        raise ValueError("invalid result cases")
    for case, labels in results.items():
        if not isinstance(labels, dict) or set(labels) != {"base", "candidate"}:
            raise ValueError("invalid revision labels")
        for data in labels.values():
            if not isinstance(data, dict) or set(data) != {"samples", "error"}:
                raise ValueError("invalid case data")
            error, batches = data["error"], data["samples"]
            if error is not None and (not isinstance(error, str) or error not in ERRORS):
                raise ValueError("invalid error")
            if not isinstance(batches, list):
                raise ValueError("invalid process samples")
            if error is not None:
                if batches:
                    raise ValueError("failed cases must not masquerade as complete")
                continue
            count = 101 if case in PREPARE else 11
            if len(batches) != report["rounds"]:
                raise ValueError("incomplete rounds")
            for batch in batches:
                if not isinstance(batch, list) or len(batch) != count or not all(map(positive, batch)):
                    raise ValueError("invalid process distribution")
    return report


def escaped(value):
    # Do not allow mentions, links, HTML or Markdown tables from an untrusted artifact.
    return re.sub(r"[^a-zA-Z0-9 ._()+:/=,-]", "?", value)[:300]


def distribution(batches):
    medians = [statistics.median(batch) for batch in batches]
    p95s = [sorted(batch)[math.ceil(len(batch) * .95) - 1] for batch in batches]
    return statistics.median(medians), statistics.median(p95s), min(medians), max(medians)


def duration(ns):
    if ns >= 1e6:
        return f"{ns / 1e6:.3f} ms"
    if ns >= 1e3:
        return f"{ns / 1e3:.3f} µs"
    return f"{ns:.2f} ns"


def render_report(report):
    validate_report(report)
    lines = [
        "## Performance changes (advisory)",
        "",
        "**Informational only — performance changes do not block merging or affect quality.**",
        "",
        f"Base: `{report['base_sha']}` · PR head: `{report['head_sha']}`",
        f"Measured candidate merge: `{report['merge_sha']}`",
        "",
        "| Workload | Base median | Candidate median | Time change | Base p95 | Candidate p95 |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    details = []
    for case, name in CASES.items():
        data = report["results"][case]
        if any(row["error"] for row in data.values()):
            reasons = "; ".join(f"{label}: {row['error']}" for label, row in data.items()
                                if row["error"])
            lines.append(f"| {name} | — | — | Unavailable ({reasons}) | — | — |")
            continue
        before, after = (distribution(data[label]["samples"]) for label in ("base", "candidate"))
        change = (after[0] / before[0] - 1) * 100
        lines.append(f"| {name} | {duration(before[0])} | {duration(after[0])} | "
                     f"{change:+.1f}% | {duration(before[1])} | {duration(after[1])} |")
        details.append(f"| {name} | {duration(before[2])}–{duration(before[3])} | "
                       f"{duration(after[2])}–{duration(after[3])} |")
    environment = report["environment"]
    lines += [
        "",
        "Positive time change = slower; negative = faster. No pass/fail threshold is applied.",
        f"Same Windows runner, release/default features, {report['rounds']} processes per revision per stage, "
        "alternating base/candidate order; builds finish before timing.",
        "Warmups excluded. Medians and p95s are summarized per process, then their medians are shown. "
        "Core update samples are batch averages, **not input/frame tail latency**.",
        "Hosted-runner noise is not calibrated: small changes are observations, not confirmed regressions.",
        "",
        "Grid: 24×80; resize source 24×160 with up to 4000 history rows / 4 MiB. "
        "Text prepare: DX12 WARP, Consolas 16px, scale 1; GPU completion excluded from timer.",
        "**Not measured:** native input/output-to-present, ConPTY resize latency, physical GPU "
        "performance, process/GPU residency, idle power or native window lifecycle.",
        "",
        f"OS: {escaped(environment['os'])}; CPU: {escaped(environment['cpu'])}; "
        f"Rust: {escaped(environment['rustc'])}.",
        f"Shared probe SHA-256: `{report['probe_sha256']}`.",
        "",
        "<details><summary>Between-process median ranges (noise visibility)</summary>",
        "",
        "| Workload | Base range | Candidate range |",
        "| --- | ---: | ---: |",
        *details,
        "",
        "</details>",
    ]
    return "\n".join(lines) + "\n"


def command(args, *, cwd=None, timeout=900, log=None):
    result = subprocess.run(args, cwd=cwd, capture_output=True, text=True,
                            encoding="utf-8", errors="replace", timeout=timeout, check=False)
    if log:
        log.write_text(result.stdout + result.stderr, encoding="utf-8")
    if result.returncode:
        print(f"Command diagnostic: {(result.stdout + result.stderr)[-8000:]!r}", file=sys.stderr)
        raise RuntimeError(f"command exited {result.returncode}; see {log}")
    return result.stdout


def build(root, out, label, probe):
    """Use exactly the same public-API probe for both source trees."""
    example = root / "crates/harbor-terminal/examples/pr_performance.rs"
    original = example.read_bytes() if example.exists() else None
    try:
        example.parent.mkdir(parents=True, exist_ok=True)
        example.write_bytes(probe)
        command(["cargo", "build", "--locked", "--release", "-p", "harbor-terminal",
                 "--example", "pr_performance", "--target-dir", str(out / "build")],
                cwd=root, log=out / f"build-{label}.log")
        suffix = ".exe" if os.name == "nt" else ""
        source = out / "build/release/examples" / ("pr_performance" + suffix)
        directory = out / label
        directory.mkdir(parents=True, exist_ok=True)
        binary = directory / ("pr_performance" + suffix)
        shutil.copy2(source, binary)
        # Stage ConPTY DLLs if the linked crate supplies them beside examples.
        for dll in source.parent.glob("*.dll"):
            shutil.copy2(dll, directory / dll.name)
        return binary
    finally:
        if original is None:
            example.unlink(missing_ok=True)
        else:
            example.write_bytes(original)


def measure(args):
    event = json.loads(args.event.read_text(encoding="utf-8"))
    pr = event["pull_request"]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    probe = Path(__file__).with_name("performance_probe.rs").read_bytes()
    report = {
        "schema": SCHEMA, "pr": event["number"],
        "run_id": int(os.environ["GITHUB_RUN_ID"]),
        "run_number": int(os.environ["GITHUB_RUN_NUMBER"]),
        "attempt": int(os.environ["GITHUB_RUN_ATTEMPT"]),
        "base_sha": pr["base"]["sha"], "head_sha": pr["head"]["sha"],
        "merge_sha": os.environ["GITHUB_SHA"],
        "probe_sha256": hashlib.sha256(probe).hexdigest(), "rounds": args.rounds,
        "environment": {
            "os": platform.platform(),
            "cpu": os.environ.get("PROCESSOR_IDENTIFIER", platform.machine()),
            "rustc": command(["rustc", "--version"]).strip(),
        },
        "results": {case: {label: {"samples": [], "error": "not measured"}
                           for label in ("base", "candidate")} for case in CASES},
    }

    def save():
        # An interrupted stage must not expose incomplete samples as a comparison.
        snapshot = json.loads(json.dumps(report, allow_nan=False))
        for labels in snapshot["results"].values():
            for data in labels.values():
                if data["error"] is not None:
                    data["samples"] = []
        temporary = out / "report.tmp"
        temporary.write_text(json.dumps(snapshot, allow_nan=False), encoding="utf-8")
        temporary.replace(out / "report.json")

    save()  # Preserve an honest unavailable report if a later build is interrupted.
    binaries = {}
    for label, root in (("base", args.base.resolve()), ("candidate", args.candidate.resolve())):
        try:
            sha = command(["git", "rev-parse", "HEAD"], cwd=root).strip()
            if sha != report["base_sha" if label == "base" else "merge_sha"]:
                raise ValueError("checkout does not match event SHA")
            binaries[label] = build(root, out, label, probe)
        except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
            print(f"{label} build unavailable: {error}", file=sys.stderr)
            for case in CASES:
                report["results"][case][label]["error"] = "build failed"
        save()

    # Keep collecting other stages even if one adapter or revision is unavailable.
    for stage, cases in STAGES.items():
        failed = set()
        for iteration in range(args.rounds):
            order = ("base", "candidate") if iteration % 2 == 0 else ("candidate", "base")
            for label in order:
                if label not in binaries or label in failed:
                    continue
                try:
                    output = command([str(binaries[label]), stage], timeout=180,
                                     log=out / f"{stage}-{label}-{iteration}.log")
                    samples = parse_output(output, stage)
                    for case in cases:
                        report["results"][case][label]["samples"].append(samples[case])
                        # Complete only after all required processes have finished.
                except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
                    failed.add(label)
                    reason = "invalid measurement" if isinstance(error, ValueError) else "measurement failed"
                    print(f"{stage}/{label} unavailable: {error}", file=sys.stderr)
                    for case in cases:
                        report["results"][case][label] = {"samples": [], "error": reason}
                save()
        for label in binaries:
            if label not in failed:
                for case in cases:
                    report["results"][case][label]["error"] = None
        save()
    body = render_report(report)
    print(body)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a", encoding="utf-8") as stream:
            stream.write(body)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--event", type=Path, required=True)
    parser.add_argument("--rounds", type=int, choices=range(2, 9), default=ROUNDS)
    measure(parser.parse_args())


if __name__ == "__main__":
    main()
