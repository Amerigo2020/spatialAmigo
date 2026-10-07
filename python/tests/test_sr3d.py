"""Sr3D adapter on a tiny fake ScanNet scan (no real data needed)."""

import json

import numpy as np
import pytest

from spatialamigo import sr3d


def box_points(lo, hi, n=20, seed=0):
    rng = np.random.default_rng(seed)
    pts = rng.uniform(lo, hi, size=(n, 3))
    # make sure the extremes are present so the box is exact
    return np.vstack([pts, lo, hi])


OBJECTS = [  # objectId, label, lo, hi
    (0, "table", [0.0, 0.0, 0.0], [1.0, 1.0, 0.7]),
    (1, "cup", [0.2, 0.2, 0.7], [0.3, 0.3, 0.8]),
    (2, "cup", [3.0, 3.0, 0.0], [3.1, 3.1, 0.1]),
    (3, "door", [-2.0, 0.0, 0.0], [-1.9, 0.9, 2.0]),
    (4, "chair", [-1.5, 0.2, 0.0], [-1.0, 0.7, 0.9]),
    (5, "chair", [4.0, 0.2, 0.0], [4.5, 0.7, 0.9]),
    (6, "box", [1.5, 0.0, 0.0], [1.9, 0.4, 0.4]),
    (7, "box", [0.0, 5.0, 0.0], [0.4, 5.4, 0.4]),
    (8, "lamp", [3.0, 0.0, 0.0], [3.3, 0.3, 1.5]),
]


@pytest.fixture
def scannet(tmp_path):
    scan_id = "scene0000_00"
    d = tmp_path / scan_id
    d.mkdir()
    pts, seg_indices, groups = [], [], []
    for k, (oid, label, lo, hi) in enumerate(OBJECTS):
        p = box_points(np.array(lo), np.array(hi), seed=k)
        pts.append(p)
        seg_indices += [100 + k] * len(p)
        groups.append({"objectId": oid, "label": label, "segments": [100 + k]})
    pts = np.vstack(pts)
    # shift everything by +10 in x; axisAlignment shifts it back
    shifted = pts + np.array([10.0, 0.0, 0.0])
    header = (
        "ply\nformat binary_little_endian 1.0\n"
        f"element vertex {len(shifted)}\n"
        "property float x\nproperty float y\nproperty float z\n"
        "property uchar red\nproperty uchar green\nproperty uchar blue\nproperty uchar alpha\n"
        "element face 0\nproperty list uchar int vertex_indices\nend_header\n"
    )
    dtype = np.dtype([("x", "<f4"), ("y", "<f4"), ("z", "<f4"), ("r", "u1"), ("g", "u1"), ("b", "u1"), ("a", "u1")])
    arr = np.zeros(len(shifted), dtype=dtype)
    arr["x"], arr["y"], arr["z"] = shifted.T
    (d / f"{scan_id}_vh_clean_2.ply").write_bytes(header.encode() + arr.tobytes())
    (d / f"{scan_id}_vh_clean_2.0.010000.segs.json").write_text(json.dumps({"segIndices": seg_indices}))
    (d / f"{scan_id}.aggregation.json").write_text(json.dumps({"sceneId": f"scannet.{scan_id}", "segGroups": groups}))
    align = np.eye(4)
    align[0, 3] = -10.0
    (d / f"{scan_id}.txt").write_text("axisAlignment = " + " ".join(f"{v:g}" for v in align.flatten()) + "\n")
    csv = tmp_path / "sr3d.csv"
    csv.write_text(
        "scan_id,target_id,distractor_ids,utterance,stimulus_id,coarse_reference_type,reference_type,instance_type,anchors_types,anchor_ids\n"
        f'{scan_id},1,[2],the cup that is supported by the table,{scan_id}-cup-2-1-2,support,supported-by,cup,[\'table\'],[0]\n'
        f'{scan_id},4,[5],the chair that is closest to the door,{scan_id}-chair-2-4-5,horizontal,closest,chair,[\'door\'],[3]\n'
        f'{scan_id},6,[7],the box that is between the table and the lamp,{scan_id}-box-2-6-7,between,between,box,"[\'table\', \'lamp\']","[0, 8]"\n'
        f'{scan_id},4,[5],the chair on the left of the door,{scan_id}-chair-2-4-5,allocentric,left,chair,[\'door\'],[3]\n'
    )
    return tmp_path, scan_id, csv


def test_build_scene_applies_alignment(scannet):
    root, scan_id, _ = scannet
    scene = sr3d.build_scene(root / scan_id, scan_id)
    table = next(o for o in scene["objects"] if o["id"] == "0")
    assert table["label"] == "table"
    assert table["center"] == pytest.approx([0.5, 0.5, 0.35], abs=1e-5)
    assert table["size"] == pytest.approx([1.0, 1.0, 0.7], abs=1e-5)
    import spatialamigo as sa

    sa.Scene(scene)  # validates


def test_read_csv(scannet):
    _, _, csv = scannet
    rows = sr3d.read_sr3d_csv(csv)
    assert [r.relation for r in rows] == ["on", "closest", "between", "left"]
    assert rows[2].anchor_ids == ["0", "8"]
    assert rows[0].distractor_ids == ["2"]
    assert rows[3].family == "allocentric"


def test_evaluate_end_to_end(scannet):
    root, scan_id, csv = scannet
    scene = sr3d.build_scene(root / scan_id, scan_id)
    rows = sr3d.read_sr3d_csv(csv)
    report = sr3d.evaluate(rows, {scan_id: scene})
    c = report.rows
    assert c["support"]["query_ok"] == 1 and c["support"]["parser_ok"] == 1
    assert c["horizontal_proximity"]["query_ok"] == 1 and c["horizontal_proximity"]["parser_ok"] == 1
    assert c["between"]["query_ok"] == 1 and c["between"]["parser_ok"] == 1
    # no orientation for the door: allocentric is skipped, not failed
    assert c["allocentric"]["skipped_n"] == 1 and c["allocentric"]["query_n"] == 0
    md = report.markdown()
    assert "| **all** | 4 | 1 | 100.0 | 100.0 |" in md


def test_fronts_enable_allocentric(scannet):
    root, scan_id, csv = scannet
    # The door faces +x (into the room); its left is +y. chair 4 is at x=-1.25, i.e. in front, so use "front".
    fronts = {"3": {"yaw": 0.0, "front": [1.0, 0.0, 0.0]}}
    scene = sr3d.build_scene(root / scan_id, scan_id, fronts=fronts)
    row = sr3d.read_sr3d_csv(csv)[3]
    row.relation = "front"
    report = sr3d.evaluate([row], {scan_id: scene})
    assert report.rows["allocentric"]["query_ok"] == 1
