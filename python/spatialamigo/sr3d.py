"""Sr3D adapter: ScanNet scans + Sr3D CSV to scene format v0.1 and evaluation.

Nothing here downloads data. ScanNet requires signing its terms of use; see
``eval/sr3d/README.md`` for the steps. Needs ``numpy`` (``pip install
spatialamigo[eval]`` or ``uv sync``).

Per scan the adapter reads, exactly like ReferIt3D does:

* ``<scan>_vh_clean_2.ply``: mesh vertices (point cloud)
* ``<scan>_vh_clean_2.0.010000.segs.json``: vertex to segment ids
* ``<scan>.aggregation.json``: segment groups = object instances with
  ``objectId`` (the Sr3D ``target_id``) and raw ``label`` (the Sr3D ``instance_type``)
* ``<scan>.txt``: ``axisAlignment`` matrix (ReferIt3D applies it as well)

Every object becomes an axis-aligned box (``yaw = 0``) around its aligned
points. ScanNet has no object orientations, so Sr3D's allocentric
utterances (which ReferIt3D generated with Scan2CAD orientations) can only be
evaluated if orientations are supplied with ``fronts`` (see
:func:`build_scene`); otherwise they are reported as skipped.
"""

from __future__ import annotations

import ast
import csv
import json
import re
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Mapping, Optional

from . import Resolver, SpatialAmigoError

try:  # numpy is an optional dependency
    import numpy as np
except ImportError:  # pragma: no cover
    np = None  # type: ignore[assignment]

REFERENCE_TYPE_TO_RELATION = {
    "closest": "closest",
    "nearest": "closest",
    "farthest": "farthest",
    "furthest": "farthest",
    "between": "between",
    "above": "above",
    "below": "below",
    "under": "below",
    "beneath": "below",
    "supported-by": "on",
    "supported_by": "on",
    "supported by": "on",
    "on": "on",
    "supporting": "under",
    "front": "front",
    "in front of": "front",
    "back": "behind",
    "behind": "behind",
    "left": "left",
    "right": "right",
}

FAMILY = {
    "closest": "horizontal_proximity",
    "farthest": "horizontal_proximity",
    "between": "between",
    "above": "vertical_proximity",
    "below": "vertical_proximity",
    "on": "support",
    "under": "support",
    "front": "allocentric",
    "behind": "allocentric",
    "left": "allocentric",
    "right": "allocentric",
}

_PLY_TYPES = {
    "char": "i1", "int8": "i1", "uchar": "u1", "uint8": "u1",
    "short": "i2", "int16": "i2", "ushort": "u2", "uint16": "u2",
    "int": "i4", "int32": "i4", "uint": "u4", "uint32": "u4",
    "float": "f4", "float32": "f4", "double": "f8", "float64": "f8",
}


def _need_numpy() -> None:
    if np is None:  # pragma: no cover
        raise ImportError("the Sr3D adapter needs numpy: uv sync, or pip install 'spatialamigo[eval]'")


def read_ply_vertices(path: Path) -> "np.ndarray":
    """Reads the vertex positions of a PLY file (ascii or binary little endian) as an (N, 3) array."""
    _need_numpy()
    with open(path, "rb") as f:
        if f.readline().strip() != b"ply":
            raise ValueError(f"{path} is not a PLY file")
        fmt, n_vertex, props, in_vertex = None, 0, [], False
        while True:
            line = f.readline()
            if not line:
                raise ValueError(f"{path}: unexpected end of header")
            tok = line.decode("ascii", errors="replace").split()
            if not tok:
                continue
            if tok[0] == "format":
                fmt = tok[1]
            elif tok[0] == "element":
                in_vertex = tok[1] == "vertex"
                if in_vertex:
                    n_vertex = int(tok[2])
            elif tok[0] == "property" and in_vertex:
                if tok[1] == "list":
                    raise ValueError(f"{path}: list properties on vertices are not supported")
                props.append((tok[2], _PLY_TYPES[tok[1]]))
            elif tok[0] == "end_header":
                break
        if fmt == "ascii":
            rows = [f.readline().split() for _ in range(n_vertex)]
            names = [p[0] for p in props]
            idx = [names.index(c) for c in ("x", "y", "z")]
            return np.array([[float(r[i]) for i in idx] for r in rows], dtype=np.float64)
        if fmt != "binary_little_endian":
            raise ValueError(f"{path}: unsupported PLY format {fmt}")
        dtype = np.dtype([(name, "<" + t) for name, t in props])
        data = np.frombuffer(f.read(dtype.itemsize * n_vertex), dtype=dtype, count=n_vertex)
        return np.stack([data["x"], data["y"], data["z"]], axis=1).astype(np.float64)


def read_axis_alignment(path: Path) -> "np.ndarray":
    """Reads ``axisAlignment = m00 m01 ... m33`` from a ScanNet ``<scan>.txt`` (identity if missing)."""
    _need_numpy()
    if path.exists():
        for line in path.read_text().splitlines():
            if line.strip().startswith("axisAlignment"):
                values = [float(v) for v in line.split("=", 1)[1].split()]
                return np.array(values, dtype=np.float64).reshape(4, 4)
    return np.eye(4)


def normalize_label(label: str) -> str:
    return " ".join(label.lower().replace("_", " ").replace("-", " ").split())


def build_scene(
    scan_dir: Path,
    scan_id: str,
    align: bool = True,
    min_points: int = 1,
    fronts: Optional[Mapping[str, Any]] = None,
) -> dict[str, Any]:
    """Builds a scene v0.1 dict from one ScanNet scan directory.

    Object ids are the ScanNet ``objectId`` as strings, labels are the
    normalized raw aggregation labels (Sr3D's ``instance_type``).
    ``fronts`` optionally maps object id to ``{"yaw": rad, "front": [x, y, z]}``
    (e.g. derived from Scan2CAD) and turns the box into an oriented one.
    """
    _need_numpy()
    scan_dir = Path(scan_dir)
    pts = read_ply_vertices(scan_dir / f"{scan_id}_vh_clean_2.ply")
    if align:
        m = read_axis_alignment(scan_dir / f"{scan_id}.txt")
        pts = pts @ m[:3, :3].T + m[:3, 3]
    segs = json.loads((scan_dir / f"{scan_id}_vh_clean_2.0.010000.segs.json").read_text())["segIndices"]
    agg = json.loads((scan_dir / f"{scan_id}.aggregation.json").read_text())
    seg_to_points: dict[int, list[int]] = defaultdict(list)
    for i, s in enumerate(segs):
        seg_to_points[s].append(i)
    objects = []
    for g in agg["segGroups"]:
        idx = [i for s in g["segments"] for i in seg_to_points.get(s, [])]
        if len(idx) < min_points:
            continue
        p = pts[idx]
        oid = str(g["objectId"])
        extra = (fronts or {}).get(oid)
        if extra:
            yaw = float(extra["yaw"])
            c, s = np.cos(-yaw), np.sin(-yaw)
            local = p[:, :2] @ np.array([[c, -s], [s, c]]).T
            lo2, hi2 = local.min(0), local.max(0)
            mid_local = (lo2 + hi2) / 2
            c2, s2 = np.cos(yaw), np.sin(yaw)
            mid = np.array([c2 * mid_local[0] - s2 * mid_local[1], s2 * mid_local[0] + c2 * mid_local[1]])
            zlo, zhi = p[:, 2].min(), p[:, 2].max()
            center = [float(mid[0]), float(mid[1]), float((zlo + zhi) / 2)]
            size = [float(max(v, 0.01)) for v in (hi2[0] - lo2[0], hi2[1] - lo2[1], zhi - zlo)]
        else:
            yaw = 0.0
            lo, hi = p.min(0), p.max(0)
            center = [float(v) for v in (lo + hi) / 2]
            size = [float(max(v, 0.01)) for v in hi - lo]
        o: dict[str, Any] = {"id": oid, "label": normalize_label(g["label"]), "center": center, "size": size, "yaw": yaw}
        if extra and extra.get("front"):
            o["front"] = [float(v) for v in extra["front"]]
        objects.append(o)
    return {"version": "0.1", "frame": "world", "objects": objects}


def _parse_list(value: Any) -> list:
    if value is None or value == "":
        return []
    if isinstance(value, list):
        return value
    try:
        out = ast.literal_eval(str(value))
        return list(out) if isinstance(out, (list, tuple)) else [out]
    except (ValueError, SyntaxError):
        return [v for v in re.split(r"[,\s]+", str(value).strip("[]")) if v]


@dataclass
class Sr3dRow:
    """One Sr3D utterance with the fields the adapter needs."""

    scan_id: str
    target_id: str
    utterance: str
    instance_type: str
    relation: Optional[str]
    reference_type: str
    anchor_ids: list[str] = field(default_factory=list)
    anchor_types: list[str] = field(default_factory=list)
    distractor_ids: list[str] = field(default_factory=list)

    @property
    def family(self) -> str:
        return FAMILY.get(self.relation or "", "unknown")


def read_sr3d_csv(path: Path, limit: Optional[int] = None) -> list[Sr3dRow]:
    """Reads the Sr3D CSV (columns as published with ReferIt3D; missing optional columns are tolerated)."""
    rows = []
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            ref = str(r.get("reference_type", "")).strip().lower()
            distractors = [str(int(float(x))) for x in _parse_list(r.get("distractor_ids"))]
            if not distractors and r.get("stimulus_id"):
                # stimulus_id: <scan_id>-<instance_type>-<n_objects>-<target_id>-<distractor ids...>
                parts = str(r["stimulus_id"]).split("-")
                distractors = [p for p in parts[4:] if p.isdigit()]
            rows.append(
                Sr3dRow(
                    scan_id=r["scan_id"],
                    target_id=str(int(float(r["target_id"]))),
                    utterance=r["utterance"],
                    instance_type=normalize_label(r.get("instance_type", "")),
                    relation=REFERENCE_TYPE_TO_RELATION.get(ref),
                    reference_type=ref,
                    anchor_ids=[str(int(float(x))) for x in _parse_list(r.get("anchor_ids"))],
                    anchor_types=[normalize_label(str(x)) for x in _parse_list(r.get("anchors_types"))],
                    distractor_ids=distractors,
                )
            )
            if limit and len(rows) >= limit:
                break
    return rows


def structured_query(row: Sr3dRow, scene: Mapping[str, Any]) -> dict[str, Any]:
    """Query built from the CSV columns (oracle language understanding): the
    candidates are the target class objects, anchors are referenced by id."""
    same = [o["id"] for o in scene["objects"] if o["label"] == row.instance_type]
    candidates = sorted(set(same) | {row.target_id} | set(row.distractor_ids), key=lambda s: int(s) if s.isdigit() else 0)
    constraint: dict[str, Any] = {"relation": row.relation, "anchors": [{"id": a} for a in row.anchor_ids]}
    if row.relation in ("left", "right", "front", "behind"):
        constraint["frame"] = "allocentric"
    return {"target": row.instance_type, "constraints": [constraint], "candidates": candidates}


@dataclass
class Sr3dReport:
    """Accuracy per relation family and mode."""

    rows: dict[str, Counter] = field(default_factory=lambda: defaultdict(Counter))
    failures: list[dict[str, Any]] = field(default_factory=list)

    def add(self, family: str, key: str, ok: bool) -> None:
        self.rows[family][key + "_n"] += 1
        self.rows[family][key + "_ok"] += int(ok)

    def markdown(self) -> str:
        lines = [
            "| Family | N | Skipped | Structured query | Template parser |",
            "| --- | ---: | ---: | ---: | ---: |",
        ]
        tot: Counter = Counter()
        for fam in sorted(self.rows):
            c = self.rows[fam]
            tot.update(c)
            lines.append(_row(fam, c))
        lines.append(_row("**all**", tot))
        return "\n".join(lines)


def _row(name: str, c: Counter) -> str:
    def pct(k: str) -> str:
        return f"{100 * c[k + '_ok'] / c[k + '_n']:.1f}" if c[k + "_n"] else "n/a"

    return f"| {name} | {c['total_n']} | {c['skipped_n']} | {pct('query')} | {pct('parser')} |"


def evaluate(
    rows: Iterable[Sr3dRow],
    scenes: Mapping[str, Mapping[str, Any]],
    max_failures: int = 50,
) -> Sr3dReport:
    """Evaluates Sr3D rows on prepared scenes in two modes:

    * structured query: relation and anchor ids from the CSV (isolates the geometry)
    * template parser: the utterance text, parsed against the scene labels
    """
    report = Sr3dReport()
    resolvers: dict[str, Resolver] = {}
    for row in rows:
        fam = row.family
        report.add(fam, "total", True)
        scene = scenes.get(row.scan_id)
        oriented = scene is not None and all(
            any(o["id"] == a and "front" in o for o in scene["objects"]) for a in row.anchor_ids
        )
        if scene is None or row.relation is None or (fam == "allocentric" and not oriented):
            report.add(fam, "skipped", True)
            continue
        r = resolvers.get(row.scan_id) or resolvers.setdefault(row.scan_id, Resolver(dict(scene)))
        try:
            ok_q = r.resolve(structured_query(row, scene)).best == row.target_id
        except SpatialAmigoError as e:
            ok_q = False
            if len(report.failures) < max_failures:
                report.failures.append({"utterance": row.utterance, "mode": "query", "error": str(e)})
        report.add(fam, "query", ok_q)
        try:
            res = r.resolve(row.utterance)
            ok_p = res.best == row.target_id
            if not ok_p and len(report.failures) < max_failures:
                report.failures.append({"utterance": row.utterance, "mode": "parser", "got": res.best, "query": res.query})
        except SpatialAmigoError as e:
            ok_p = False
            if len(report.failures) < max_failures:
                report.failures.append({"utterance": row.utterance, "mode": "parser", "error": str(e)})
        report.add(fam, "parser", ok_p)
    return report
