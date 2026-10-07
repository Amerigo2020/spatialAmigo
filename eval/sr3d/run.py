"""Build v0.1 scenes from ScanNet for the scans used by Sr3D and evaluate spatialAmigo.

    uv run python eval/sr3d/run.py --scannet data/scannet/scans --sr3d data/sr3d.csv --out eval/sr3d/out

See eval/sr3d/README.md for how to obtain the data (ScanNet needs a signed
terms-of-use agreement; nothing is downloaded by this script).
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

from spatialamigo import sr3d


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scannet", type=Path, required=True, help="ScanNet 'scans' directory (one folder per scan)")
    ap.add_argument("--sr3d", type=Path, required=True, help="Sr3D CSV from ReferIt3D")
    ap.add_argument("--out", type=Path, default=Path("eval/sr3d/out"))
    ap.add_argument("--limit", type=int, default=None, help="only the first N utterances")
    ap.add_argument("--fronts", type=Path, default=None, help="optional JSON {scan_id: {object_id: {yaw, front}}}")
    ap.add_argument("--no-align", action="store_true", help="skip the ScanNet axis alignment")
    ap.add_argument("--scans-file", type=Path, default=None, help="optional text file with one scan id per line (e.g. a test split)")
    args = ap.parse_args()

    rows = sr3d.read_sr3d_csv(args.sr3d, None if args.scans_file else args.limit)
    if args.scans_file:
        keep = {line.strip() for line in args.scans_file.read_text().splitlines() if line.strip()}
        rows = [r for r in rows if r.scan_id in keep][: args.limit]
    fronts = json.loads(args.fronts.read_text()) if args.fronts else {}
    scan_ids = sorted({r.scan_id for r in rows})
    scene_dir = args.out / "scenes"
    scene_dir.mkdir(parents=True, exist_ok=True)
    scenes, missing = {}, []
    t0 = time.time()
    for i, scan_id in enumerate(scan_ids, 1):
        cached = scene_dir / f"{scan_id}.json"
        if cached.exists() and not args.fronts:
            scenes[scan_id] = json.loads(cached.read_text())
            continue
        src = args.scannet / scan_id
        if not src.exists():
            missing.append(scan_id)
            continue
        scene = sr3d.build_scene(src, scan_id, align=not args.no_align, fronts=fronts.get(scan_id))
        cached.write_text(json.dumps(scene))
        scenes[scan_id] = scene
        if i % 50 == 0:
            print(f"  built {i}/{len(scan_ids)} scenes ({time.time() - t0:.0f} s)", file=sys.stderr)
    if missing:
        print(f"warning: {len(missing)} scans not found under {args.scannet} (e.g. {missing[:3]})", file=sys.stderr)

    t1 = time.time()
    report = sr3d.evaluate(rows, scenes)
    dt = time.time() - t1
    md = report.markdown()
    print(md)
    print(f"\n{len(rows)} utterances, {len(scenes)} scenes, evaluated in {dt:.1f} s")
    (args.out / "report.md").write_text(md + "\n")
    (args.out / "failures.json").write_text(json.dumps(report.failures, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
