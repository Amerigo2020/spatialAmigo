"""Turns criterion results (target/criterion) into the Markdown table used in the README.

    cargo bench -p spatialamigo --bench scoring
    python3 bench/criterion_table.py > results/perf/criterion.md
"""

import json
import platform
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "target" / "criterion"
GROUPS = [
    ("resolve/closest", "closest"),
    ("resolve/on", "on"),
    ("resolve/left", "left"),
    ("resolve/between", "between"),
    ("resolve/on+left+closest", "on AND left AND closest"),
    ("resolve_no_text/closest", "closest, no text explanation"),
]
SIZES = [10, 100, 1000, 10000]


def median_ns(group: str, n: int) -> float:
    path = ROOT / group.replace("/", "_") / str(n) / "new" / "estimates.json"
    if not path.exists():
        path = ROOT / group / str(n) / "new" / "estimates.json"
    return json.loads(path.read_text())["median"]["point_estimate"]


def cpu() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown CPU"


def fmt(ns: float) -> str:
    return f"{ns / 1e3:.1f} µs" if ns < 1e6 else f"{ns / 1e6:.2f} ms"


def main() -> None:
    print("| Query | " + " | ".join(f"N = {n}" for n in SIZES) + " | per candidate at N = 1000 |")
    print("| --- |" + " ---: |" * (len(SIZES) + 1))
    for group, name in GROUPS:
        vals = [median_ns(group, n) for n in SIZES]
        print(f"| {name} | " + " | ".join(fmt(v) for v in vals) + f" | {vals[2] / 1000:.0f} ns |")
    print(f"\nCriterion medians, full resolution including the per-candidate trace. {cpu()}, {platform.system()}.")


if __name__ == "__main__":
    main()
