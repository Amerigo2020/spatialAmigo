# Evaluating spatialAmigo on Sr3D (ReferIt3D)

This folder contains everything needed to run spatialAmigo on the **Sr3D** split of ReferIt3D (Achlioptas et al., ECCV 2020) on top of ScanNet. **No data is included or downloaded automatically.** ScanNet may only be used after accepting its Terms of Use, so the steps below have to be run by a person who has been granted access.

## What is measured

Sr3D utterances refer to an object in a real ScanNet scan through one anchor relation ("the chair that is closest to the door"). The adapter turns every scan into scene format v0.1 (one axis-aligned box per annotated object, labels from the ScanNet aggregation, exactly the instance labels Sr3D uses) and reports top-1 accuracy per relation family in two modes:

| Mode | Input to spatialAmigo | Isolates |
| --- | --- | --- |
| Structured query | relation, anchor ids and target class from the CSV columns | the geometric relation scoring |
| Template parser | the raw utterance text, parsed against the scene labels | parser plus geometry |

Important: the adapter uses **ground-truth boxes and labels** from the ScanNet annotation. The numbers therefore measure spatial reasoning with perfect perception. They are not comparable to ReferIt3D listeners that work on raw point clouds and have to recognise objects first.

Allocentric utterances need object orientations, which ScanNet does not contain (ReferIt3D took them from Scan2CAD). Without `--fronts` these rows are counted as **skipped**, not as failures.

## Steps

1. **Request ScanNet access.** Read and sign the ScanNet Terms of Use (linked from http://www.scan-net.org/ and https://github.com/ScanNet/ScanNet) and send the form as instructed there. You will receive the download script `download-scannet.py`.

2. **Download only the four files per scan the adapter needs** (a few GB instead of the full 1.3 TB):

   ```bash
   mkdir -p data/scannet
   for t in _vh_clean_2.ply _vh_clean_2.0.010000.segs.json .aggregation.json .txt; do
     python download-scannet.py -o data/scannet --type "$t"
   done
   ```

   Afterwards `data/scannet/scans/scene0000_00/` contains `scene0000_00_vh_clean_2.ply`, `scene0000_00_vh_clean_2.0.010000.segs.json`, `scene0000_00.aggregation.json` and `scene0000_00.txt`.

3. **Download Sr3D.** The ReferIt3D README (https://github.com/referit3d/referit3d) links the Sr3D and Sr3D+ CSV files (Google Drive). Save `sr3d.csv` as `data/sr3d.csv`. Optionally take the test split scan list from the ReferIt3D repository and save it as `data/sr3d_test_scans.txt` (one scan id per line).

4. **Run the evaluation** from the repository root:

   ```bash
   uv sync
   uv run python eval/sr3d/run.py --scannet data/scannet/scans --sr3d data/sr3d.csv --out eval/sr3d/out
   # only the test scans, or a quick smoke run:
   uv run python eval/sr3d/run.py --scannet data/scannet/scans --sr3d data/sr3d.csv --scans-file data/sr3d_test_scans.txt
   uv run python eval/sr3d/run.py --scannet data/scannet/scans --sr3d data/sr3d.csv --limit 500
   ```

   Output: `eval/sr3d/out/report.md` (table per family and mode), `eval/sr3d/out/failures.json` (first 50 failures with the parsed query) and the converted scenes in `eval/sr3d/out/scenes/`. Converted scenes are cached, so later runs are fast. `data/` and `eval/sr3d/out/` are git-ignored because they are derived from ScanNet; do not commit or publish them.

5. **Optional: allocentric relations.** `--fronts` takes object orientations as JSON `{scan_id: {object_id: {"yaw": rad, "front": [1, 0, 0]}}}` (`yaw` is the heading of the front in the aligned scan, the box is oriented along it). `eval/sr3d/scan2cad_to_fronts.py` writes this file from one of two sources:

   * **ReferIt3D's own oriented boxes (recommended, exact).** The ReferIt3D repository contains `referit3d/data/scan2cad/object_oriented_bboxes/object_oriented_bboxes_aligned_scans.json` (keyed by `<scan>_<objectId>`, aligned coordinates, a `front_point` on the front face) and `referit3d/data/scan2cad/shapenet_has_front.csv` (which ShapeNet models have a front; ReferIt3D annotated this for Sr3D). These are the fronts Sr3D was generated with. They are derived from Scan2CAD, so check that its terms cover your use.

     ```bash
     uv run python eval/sr3d/scan2cad_to_fronts.py referit3d --boxes object_oriented_bboxes_aligned_scans.json          --has-front shapenet_has_front.csv --out eval/sr3d/fronts.json
     ```

   * **Scan2CAD `full_annotations.json`** (https://github.com/skanti/Scan2CAD, requires its own agreement). Each CAD alignment is transformed into the axis-aligned ScanNet frame (`axisAlignment @ inv(M_scan) @ M_cad`, `M = T @ R @ S`, quaternions `[w, x, y, z]`, as in Scan2CAD's `Annotation2Mesh.py`) and assigned to the ScanNet instance whose axis-aligned box has the highest IoU with the transformed CAD box (`--min-iou`, default 0.3; one alignment per object, best IoU wins). The front is the CAD `-z` axis (ShapeNet: y up, front `-z`). Symmetric models (`sym` other than `__SYM_NONE`) and fronts tilted more than 60° out of the floor are skipped. Passing ReferIt3D's `--has-front` CSV restricts the result to models with a front, as in Sr3D.

     ```bash
     uv run python eval/sr3d/scan2cad_to_fronts.py scan2cad --annotations full_annotations.json          --scannet data/scannet/scans --has-front shapenet_has_front.csv --out eval/sr3d/fronts.json
     ```

   Then add `--fronts eval/sr3d/fronts.json` to `run.py`. The script prints how many alignments were matched, symmetric, unmatched or tilted.

## Column mapping

| Sr3D column | Used as |
| --- | --- |
| `scan_id` | scan folder |
| `target_id` | ScanNet `objectId` of the referred object (ground truth) |
| `distractor_ids` | same-class objects (fallback: decoded from `stimulus_id`) |
| `utterance` | input of the template parser |
| `reference_type` | relation (`closest`, `farthest`, `between`, `above`, `below`, `supported-by` to `on`, `supporting` to `under`, `front`, `back` to `behind`, `left`, `right`) |
| `instance_type` | target class |
| `anchor_ids` | anchors by id in the structured-query mode |

Unknown reference types are skipped and show up in the `Skipped` column. The adapter is tested on a synthetic mini scan in `python/tests/test_sr3d.py`.
