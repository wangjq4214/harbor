"""Summarize per-process medians from the alternating hot-path timing runs."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import re
import statistics

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", type=Path, default=Path("target/hot-path-timing/results"))
    args = parser.parse_args()
    groups = defaultdict(lambda: defaultdict(list))
    for path in sorted(args.results.glob("*.log")):
        label = "before" if "-before-" in path.name else "after"
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.startswith("TIMING "):
                continue
            stage = line.split()[1]
            fields = dict(re.findall(r"(\w+)=([^\s]+)", line.split(" samples_ns=")[0]))
            if stage == "reader":
                handoff = fields.get("handoff", "single")
                scenario = f"reader {handoff} {fields['chunk']}B batch={fields['batch']}"
            elif stage == "engine":
                scenario = f"engine {fields['rows']}x{fields['cols']} sessions={fields['sessions']} {fields['mode']}"
            else:
                scenario = f"sequence {fields['rows']}x80 {fields['fixture']} {fields['mode']}"
            groups[scenario][label].append({
                "median_ns": float(fields["median_ns"]),
                "p95_ns": float(fields.get("p95_ns", fields.get("batch_p95_ns"))),
                "log": path.name,
            })
    summaries = []
    table = ["| Scenario | Before ns | After ns | Time change |",
             "| --- | ---: | ---: | ---: |"]
    for scenario, labels in sorted(groups.items()):
        if set(labels) != {"before", "after"}:
            raise ValueError(f"incomplete A/B evidence: {scenario}")
        summary = {"scenario": scenario}
        for label in ("before", "after"):
            medians = [sample["median_ns"] for sample in labels[label]]
            summary[label] = {
                "processes": len(medians),
                "median_ns": statistics.median(medians),
                "median_range_ns": [min(medians), max(medians)],
                "median_p95_ns": statistics.median(sample["p95_ns"] for sample in labels[label]),
                "process_results": labels[label],
            }
        before, after = (summary[label]["median_ns"] for label in ("before", "after"))
        summary["time_change_percent"] = (after / before - 1) * 100
        summaries.append(summary)
        table.append(f"| {scenario} | {before:.2f} | {after:.2f} | {summary['time_change_percent']:+.1f}% |")
    (args.results / "summary.json").write_text(json.dumps(summaries, indent=2), encoding="utf-8")
    (args.results / "summary.md").write_text("\n".join(table) + "\n", encoding="utf-8")
    print("\n".join(table))

if __name__ == "__main__":
    main()
