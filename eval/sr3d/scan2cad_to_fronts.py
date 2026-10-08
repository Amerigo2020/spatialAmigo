"""Write the --fronts JSON for eval/sr3d/run.py from Scan2CAD or ReferIt3D annotations.

    # Scan2CAD alignments, matched to ScanNet objectIds by box IoU
    uv run python eval/sr3d/scan2cad_to_fronts.py scan2cad --annotations data/scan2cad/full_annotations.json \
        --scannet data/scannet/scans --out eval/sr3d/fronts.json [--has-front shapenet_has_front.csv]

    # ReferIt3D's own oriented boxes (the fronts Sr3D was generated with)
    uv run python eval/sr3d/scan2cad_to_fronts.py referit3d \
        --boxes object_oriented_bboxes_aligned_scans.json --has-front shapenet_has_front.csv --out eval/sr3d/fronts.json

Conventions and sources: python/spatialamigo/scan2cad.py. Nothing is downloaded.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from spatialamigo import scan2cad


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="source", required=True)
    a = sub.add_parser("scan2cad")
    a.add_argument("--annotations", type=Path, required=True, help="Scan2CAD full_annotations.json")
    a.add_argument("--scannet", type=Path, required=True, help="ScanNet 'scans' directory")
    a.add_argument("--min-iou", type=float, default=0.3)
    a.add_argument("--scans-file", type=Path, default=None, help="only these scan ids (one per line)")
    b = sub.add_parser("referit3d")
    b.add_argument("--boxes", type=Path, required=True, help="ReferIt3D object_oriented_bboxes_aligned_scans.json")
    for p in (a, b):
        p.add_argument("--has-front", type=Path, default=None, help="ReferIt3D shapenet_has_front.csv")
        p.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    has_front = scan2cad.has_front_set(args.has_front) if args.has_front else None
    if args.source == "scan2cad":
        scans = set(args.scans_file.read_text().split()) if args.scans_file else None
        annotations = json.loads(args.annotations.read_text())
        fronts, stats = scan2cad.from_scan2cad(annotations, args.scannet, args.min_iou, has_front, scans)
    else:
        fronts, stats = scan2cad.from_referit3d(json.loads(args.boxes.read_text()), has_front)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(fronts))
    print(json.dumps(stats), f"-> {sum(map(len, fronts.values()))} fronts in {len(fronts)} scans, wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
