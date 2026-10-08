"""Scan2CAD/ReferIt3D front conversion on the fake ScanNet scan from test_sr3d (no real data)."""

import math

import numpy as np
import pytest

from spatialamigo import scan2cad, sr3d
from test_sr3d import scannet  # noqa: F401  (pytest fixture)

SCAN = "scene0000_00"


def qmul(a, b):
    w1, x1, y1, z1 = a
    w2, x2, y2, z2 = b
    return [
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
    ]


def about(axis, angle):
    s = math.sin(angle / 2)
    return [math.cos(angle / 2)] + [s * v for v in axis]


THETA = 0.7  # scan-to-world rotation about z, must cancel out
SCAN_TRS = {"translation": [1.0, 2.0, 3.0], "rotation": about([0, 0, 1], THETA), "scale": [1, 1, 1]}
# CAD y-up -> world z-up and CAD front (-z) -> aligned +y
CAD_ROT = qmul(about([0, 0, 1], THETA), about([1, 0, 0], math.pi / 2))


def world(aligned_point):
    raw = np.asarray(aligned_point) + [10.0, 0.0, 0.0]  # the fixture's axisAlignment is a -10 x shift
    return list(scan2cad.trs_matrix(SCAN_TRS)[:3, :3] @ raw + SCAN_TRS["translation"])


def model(aligned_center, half_extents, sym="__SYM_NONE", catid="03001627", cad="chair_a"):
    return {
        "catid_cad": catid,
        "id_cad": cad,
        "sym": sym,
        "trs": {"translation": world(aligned_center), "rotation": CAD_ROT, "scale": [1, 1, 1]},
        "center": [0, 0, 0],
        "bbox": half_extents,
    }


@pytest.fixture
def annotations():
    return [
        {
            "id_scan": SCAN,
            "trs": SCAN_TRS,
            "aligned_models": [
                model([-1.25, 0.45, 0.45], [0.25, 0.45, 0.25]),  # chair objectId 4, CAD axes (x, up, -front)
                model([-1.24, 0.45, 0.45], [0.2, 0.4, 0.2], cad="chair_b"),  # worse duplicate of the same chair
                model([0.5, 0.5, 0.35], [0.5, 0.35, 0.5], sym="__SYM_ROTATE_UP_2", catid="04379243"),  # table
                model([8.0, 8.0, 0.5], [0.3, 0.5, 0.3]),  # nothing there
            ],
        },
        {"id_scan": "scene9999_00", "trs": SCAN_TRS, "aligned_models": []},
    ]


def test_quaternion_and_trs_follow_scan2cad():
    r = scan2cad.quat_to_matrix(about([0, 0, 1], math.pi / 2))
    assert np.allclose(r @ [1, 0, 0], [0, 1, 0])
    m = scan2cad.trs_matrix({"translation": [1, 2, 3], "rotation": [1, 0, 0, 0], "scale": [2, 3, 4]})
    assert np.allclose(m @ [1, 1, 1, 1], [3, 5, 7, 1])  # T @ R @ S: scale first


def test_scan2cad_matches_objects_and_recovers_front(scannet, annotations):  # noqa: F811
    root = scannet[0]
    fronts, stats = scan2cad.from_scan2cad(annotations, root)
    assert set(fronts) == {SCAN}
    assert set(fronts[SCAN]) == {"4"}
    entry = fronts[SCAN]["4"]
    assert entry["yaw"] == pytest.approx(math.pi / 2)
    assert entry["front"] == [1.0, 0.0, 0.0]
    assert entry["iou"] > 0.9
    assert stats == {"models": 4, "symmetric": 1, "no_front": 0, "unmatched": 2, "tilted": 0, "matched": 1, "missing_scans": 1}


def test_has_front_filter(scannet, annotations, tmp_path):  # noqa: F811
    root = scannet[0]
    csv_path = tmp_path / "shapenet_has_front.csv"
    csv_path.write_text("syn_id,model_name,has_front\n3001627,chair_a,0\n3001627,chair_b,1\n")
    hf = scan2cad.has_front_set(csv_path)
    assert hf == {("03001627", "chair_b")}  # pandas-style ids lose the leading zero
    fronts, stats = scan2cad.from_scan2cad(annotations, root, has_front=hf)
    assert set(fronts[SCAN]) == {"4"} and stats["no_front"] == 2


def test_fronts_feed_build_scene(scannet, annotations):  # noqa: F811
    root = scannet[0]
    fronts, _ = scan2cad.from_scan2cad(annotations, root)
    scene = sr3d.build_scene(root / SCAN, SCAN, fronts=fronts[SCAN])
    chair = next(o for o in scene["objects"] if o["id"] == "4")
    assert chair["yaw"] == pytest.approx(math.pi / 2) and chair["front"] == [1.0, 0.0, 0.0]
    assert chair["size"][:2] == pytest.approx([0.5, 0.5])


def test_from_referit3d():
    boxes = {
        "scene0000_00_12": {"catid_cad": "02747177", "id_cad": "a", "obj_bbox": [2.0, 1.0, 0.2, 0.2, 0.3, 0.4], "front_point": [1.9, 1.0, 0.2]},
        "scene0000_00_39": {"catid_cad": "03001627", "id_cad": "b", "obj_bbox": [0, 0, 0, 1, 1, 1], "front_point": [0, 0.5, 0]},
        "scene0001_00_3": {"catid_cad": "03001627", "id_cad": "c", "obj_bbox": [0, 0, 0, 1, 1, 1], "front_point": [0, 0, 0.5]},
    }
    fronts, stats = scan2cad.from_referit3d(boxes)
    assert fronts["scene0000_00"]["12"]["yaw"] == pytest.approx(math.pi)
    assert fronts["scene0000_00"]["39"]["yaw"] == pytest.approx(math.pi / 2)
    assert stats == {"objects": 3, "no_front": 0, "tilted": 1, "matched": 2}
    fronts, stats = scan2cad.from_referit3d(boxes, has_front={("03001627", "b")})
    assert list(fronts) == ["scene0000_00"] and stats["no_front"] == 2
