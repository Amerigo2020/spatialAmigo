"""spatialAmigo (Rust via Python binding) vs. a pure NumPy reference.

The NumPy code re-implements three relations (closest, on, left) for
axis-aligned boxes, vectorized over all candidates, with the formulas from
docs/relations.md. It serves two purposes:

1. Cross-check: the NumPy scores must match spatialAmigo's to 1e-9.
2. Timing: what a vectorized NumPy implementation costs compared to the Rust
   core called from Python. The scene is the same grid as in
   crates/spatialamigo/benches/scoring.rs.

    uv run python bench/compare_numpy.py [--out results/perf/numpy_vs_rust.md]
"""

from __future__ import annotations

import argparse
import math
import platform
import statistics
import time
from pathlib import Path

import numpy as np

import spatialamigo as sa

P = sa.default_params()


def grid_scene(n: int) -> dict:
    k = math.ceil(math.sqrt(n))
    side = k * 0.15 + 1.0
    objs = [
        {"id": "table", "label": "table", "center": [0.0, 0.0, 0.35], "size": [side, side, 0.7], "yaw": 0.0},
        {"id": "laptop", "label": "laptop", "center": [0.07, 0.11, 0.71], "size": [0.24, 0.32, 0.02], "yaw": 0.0, "front": [1.0, 0.0, 0.0]},
        {"id": "book", "label": "book", "center": [-0.4, 0.3, 0.715], "size": [0.16, 0.22, 0.03], "yaw": 0.0},
    ]
    for i in range(n):
        x = (i % k) * 0.15 - side / 2 + 0.5
        y = (i // k) * 0.15 - side / 2 + 0.5
        objs.append({"id": f"mug_{i}", "label": "mug", "center": [x, y, 0.75], "size": [0.08, 0.08, 0.1], "yaw": 0.0})
    return {"version": "0.1", "frame": "world", "objects": objs}


def arrays(scene: dict, label: str):
    objs = [o for o in scene["objects"] if o["label"] == label]
    c = np.array([o["center"] for o in objs])
    h = np.array([o["size"] for o in objs]) / 2
    return [o["id"] for o in objs], c, h


def anchor(scene: dict, oid: str):
    o = next(o for o in scene["objects"] if o["id"] == oid)
    return np.array(o["center"]), np.array(o["size"]) / 2


def footprint_gap(c, h, ca, ha):
    d = np.maximum(np.abs(c[:, :2] - ca[:2]) - (h[:, :2] + ha[:2]), 0.0)
    return np.hypot(d[:, 0], d[:, 1])


def np_closest(c, h, ca, ha):
    gap = footprint_gap(c, h, ca, ha)
    d = gap + P["center_weight"] * np.hypot(*(c[:, :2] - ca[:2]).T)
    return np.exp(-(d - d.min()) / P["proximity_tau"])


def np_on(c, h, ca, ha):
    lo = np.maximum(c[:, :2] - h[:, :2], ca[:2] - ha[:2])
    hi = np.minimum(c[:, :2] + h[:, :2], ca[:2] + ha[:2])
    inter = np.prod(np.maximum(hi - lo, 0.0), axis=1)
    support = np.clip(inter / (4 * h[:, 0] * h[:, 1]), 0, 1)
    gap = footprint_gap(c, h, ca, ha)
    xy = 0.5 * support + 0.5 * np.exp(-((gap / P["xy_sigma"]) ** 2))
    gz = (c[:, 2] - h[:, 2]) - (ca[2] + ha[2])
    return xy * np.exp(-((gz / P["contact_sigma"]) ** 2))


def sigmoid(x):
    return 1.0 / (1.0 + np.exp(-x))


def np_left(c, h, ca, ha):
    # anchor front = +x (laptop, yaw 0), left = +y
    u = c[:, 0] - ca[0]
    w = c[:, 1] - ca[1]
    eu = np.abs(u) - ha[0]
    ew = np.abs(w) - ha[1]
    eps = P["sector_smoothing"]
    lat = (ew - eu) / (np.abs(ew) + np.abs(eu) + eps)
    sector = sigmoid(lat / P["sector_softness"]) * sigmoid(w / eps)
    falloff = 1.0 / (1.0 + footprint_gap(c, h, ca, ha) / P["direction_falloff"])
    return sector * falloff


RELATIONS = {"closest": ("laptop", np_closest), "on": ("table", np_on), "left": ("laptop", np_left)}


def timeit(fn, min_time=0.3, min_runs=5):
    runs, t_end = [], time.perf_counter() + min_time
    while len(runs) < min_runs or time.perf_counter() < t_end:
        t = time.perf_counter()
        fn()
        runs.append(time.perf_counter() - t)
    return statistics.median(runs)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, default=None)
    ap.add_argument("--sizes", type=int, nargs="+", default=[10, 100, 1000, 10000])
    args = ap.parse_args()

    lines = [
        "| Relation | N | spatialAmigo via Python (µs) | same, no text explanation (µs) | NumPy reference (µs) | max abs. score diff |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    for rel, (anchor_id, fn) in RELATIONS.items():
        for n in args.sizes:
            scene = grid_scene(n)
            r = sa.Resolver(scene)
            q = {"target": "mug", "constraints": [{"relation": rel, "anchors": [anchor_id]}]}
            ids, c, h = arrays(scene, "mug")
            ca, ha = anchor(scene, anchor_id)

            ref = dict(zip(ids, fn(c, h, ca, ha)))
            got = dict(r.resolve(q).scores)
            diff = max(abs(got[i] - ref[i]) for i in ids)
            assert diff < 1e-9, f"{rel} n={n}: NumPy and Rust disagree by {diff}"

            t_full = timeit(lambda: r.resolve(q).scores)
            t_quiet = timeit(lambda: r.resolve(q, explain=False).scores)
            t_np = timeit(lambda: fn(c, h, ca, ha))
            lines.append(f"| {rel} | {n} | {t_full * 1e6:.0f} | {t_quiet * 1e6:.0f} | {t_np * 1e6:.0f} | {diff:.1e} |")
            print(lines[-1], flush=True)
    env = f"Python {platform.python_version()}, NumPy {np.__version__}, spatialAmigo {sa.__version__}, {platform.machine()} {platform.system()}"
    md = "\n".join(lines) + f"\n\nMedian wall time per call. {env}.\n"
    print("\n" + md)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(md)


if __name__ == "__main__":
    main()
