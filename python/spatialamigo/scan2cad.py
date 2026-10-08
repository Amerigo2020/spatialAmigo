"""Object fronts for Sr3D allocentric relations, from Scan2CAD or ReferIt3D.

Output format (what ``eval/sr3d/run.py --fronts`` reads)::

    {scan_id: {object_id: {"yaw": rad, "front": [1, 0, 0]}}}

``yaw`` is the heading of the object's front in the axis-aligned ScanNet frame
(z up), so the box is oriented along the front and ``front`` is the +x axis
of the object frame.

Two sources are supported:

* ``from_scan2cad``: Scan2CAD ``full_annotations.json``. Conventions, checked
  against ``Routines/Script/Annotation2Mesh.py`` in github.com/skanti/Scan2CAD:
  ``trs`` is composed as ``M = T @ R @ S``, quaternions are ``[qw, qx, qy, qz]``,
  the scan ``trs`` maps raw ``_vh_clean_2.ply`` coordinates to the Scan2CAD
  world, each model ``trs`` maps CAD coordinates to the same world, and the CAD
  box is ``center +- bbox`` (half extents) in CAD coordinates. ShapeNet CAD
  space is y up with the front facing -z (Scan2CAD's symmetry rotations are
  about +y; the backrests of the sample chairs in its repository lie at +z).
  Alignments are matched to ScanNet ``objectId`` s by the IoU of axis-aligned
  boxes after ScanNet's ``axisAlignment``. Symmetric models (any ``sym`` other
  than ``__SYM_NONE``) have no unique front and are skipped.
* ``from_referit3d``: the ``object_oriented_bboxes_aligned_scans.json`` that
  ReferIt3D used to generate Sr3D (already keyed by ``<scan>_<objectId>``, in
  aligned coordinates, with a ``front_point`` on the front face), filtered by
  their ``shapenet_has_front.csv``. This reproduces Sr3D's fronts exactly.
"""

from __future__ import annotations

import csv
import math
from pathlib import Path
from typing import Any, Iterable, Mapping, Optional

from .sr3d import _need_numpy, instance_points, read_axis_alignment

try:
    import numpy as np
except ImportError:  # pragma: no cover
    np = None  # type: ignore[assignment]

CAD_FRONT = (0.0, 0.0, -1.0)
MIN_HORIZONTAL = 0.5  # skip fronts tilted more than 60 degrees out of the floor plane


def quat_to_matrix(q: Iterable[float]) -> "np.ndarray":
    """Rotation matrix from a quaternion ``[w, x, y, z]`` (normalized first)."""
    w, x, y, z = np.asarray(list(q), dtype=np.float64) / np.linalg.norm(list(q))
    return np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )


def trs_matrix(trs: Mapping[str, Any]) -> "np.ndarray":
    """4x4 ``T @ R @ S`` as in Scan2CAD's ``make_M_from_tqs``."""
    m = np.eye(4)
    m[:3, :3] = quat_to_matrix(trs["rotation"]) @ np.diag(np.asarray(trs["scale"], dtype=np.float64))
    m[:3, 3] = trs["translation"]
    return m


def aabb_iou(a: tuple, b: tuple) -> float:
    """IoU of two axis-aligned boxes given as ``(lo, hi)``."""
    inter = np.prod(np.clip(np.minimum(a[1], b[1]) - np.maximum(a[0], b[0]), 0, None))
    vol = lambda box: np.prod(np.asarray(box[1]) - np.asarray(box[0]))  # noqa: E731
    union = vol(a) + vol(b) - inter
    return float(inter / union) if union > 0 else 0.0


def front_entry(direction: Iterable[float]) -> Optional[dict[str, Any]]:
    """``{"yaw", "front"}`` for a world-frame front direction, or None if it is not horizontal enough."""
    d = np.asarray(list(direction), dtype=np.float64)
    n = np.linalg.norm(d)
    h = math.hypot(d[0], d[1])
    if n == 0 or h / n < MIN_HORIZONTAL:
        return None
    return {"yaw": math.atan2(d[1], d[0]), "front": [1.0, 0.0, 0.0]}


def has_front_set(path: Path) -> set[tuple[str, str]]:
    """``(catid, model id)`` pairs marked ``has_front == 1`` in ReferIt3D's ``shapenet_has_front.csv``."""
    with open(path, newline="") as f:
        return {(r["syn_id"].zfill(8), r["model_name"]) for r in csv.DictReader(f) if r["has_front"].strip() == "1"}


def from_scan2cad(
    annotations: list[Mapping[str, Any]],
    scannet_dir: Path,
    min_iou: float = 0.3,
    has_front: Optional[set[tuple[str, str]]] = None,
    scan_ids: Optional[set[str]] = None,
) -> tuple[dict[str, dict[str, Any]], dict[str, int]]:
    """Fronts from Scan2CAD alignments. Returns ``(fronts, stats)``."""
    _need_numpy()
    fronts: dict[str, dict[str, Any]] = {}
    stats = {"models": 0, "symmetric": 0, "no_front": 0, "unmatched": 0, "tilted": 0, "matched": 0, "missing_scans": 0}
    for ann in annotations:
        scan_id = ann["id_scan"]
        if scan_ids is not None and scan_id not in scan_ids:
            continue
        scan_dir = Path(scannet_dir) / scan_id
        if not scan_dir.exists():
            stats["missing_scans"] += 1
            continue
        align = read_axis_alignment(scan_dir / f"{scan_id}.txt")
        boxes = {oid: (p.min(0), p.max(0)) for oid, _, p in instance_points(scan_dir, scan_id, align=True)}
        to_aligned = align @ np.linalg.inv(trs_matrix(ann["trs"]))
        candidates = []  # (iou, model index, object id, front direction)
        for k, model in enumerate(ann["aligned_models"]):
            stats["models"] += 1
            if model.get("sym", "__SYM_NONE") != "__SYM_NONE":
                stats["symmetric"] += 1
                continue
            if has_front is not None and (model["catid_cad"], model["id_cad"]) not in has_front:
                stats["no_front"] += 1
                continue
            m = to_aligned @ trs_matrix(model["trs"])
            half, center = np.asarray(model["bbox"], dtype=np.float64), np.asarray(model["center"], dtype=np.float64)
            signs = np.array([[sx, sy, sz] for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)], dtype=np.float64)
            corners = (center + signs * half) @ m[:3, :3].T + m[:3, 3]
            cad_box = (corners.min(0), corners.max(0))
            best = max(((aabb_iou(cad_box, box), oid) for oid, box in boxes.items()), default=(0.0, None))
            if best[0] < min_iou:
                stats["unmatched"] += 1
                continue
            candidates.append((best[0], k, best[1], m[:3, :3] @ np.array(CAD_FRONT)))
        # one alignment per object: keep the best-overlapping one
        taken: set[str] = set()
        for iou, _, oid, direction in sorted(candidates, key=lambda c: -c[0]):
            if oid in taken:
                stats["unmatched"] += 1
                continue
            entry = front_entry(direction)
            if entry is None:
                stats["tilted"] += 1
                continue
            taken.add(oid)
            fronts.setdefault(scan_id, {})[oid] = {**entry, "iou": round(iou, 3)}
            stats["matched"] += 1
    return fronts, stats


def from_referit3d(
    oriented_boxes: Mapping[str, Mapping[str, Any]],
    has_front: Optional[set[tuple[str, str]]] = None,
) -> tuple[dict[str, dict[str, Any]], dict[str, int]]:
    """Fronts from ReferIt3D's ``object_oriented_bboxes_aligned_scans.json``."""
    _need_numpy()
    fronts: dict[str, dict[str, Any]] = {}
    stats = {"objects": 0, "no_front": 0, "tilted": 0, "matched": 0}
    for key, info in oriented_boxes.items():
        stats["objects"] += 1
        scan_id, oid = key[:12], key[13:]
        if has_front is not None and (info["catid_cad"], info["id_cad"]) not in has_front:
            stats["no_front"] += 1
            continue
        entry = front_entry(np.asarray(info["front_point"]) - np.asarray(info["obj_bbox"][:3]))
        if entry is None:
            stats["tilted"] += 1
            continue
        fronts.setdefault(scan_id, {})[oid] = entry
        stats["matched"] += 1
    return fronts, stats
