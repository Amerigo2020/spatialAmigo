"""The example scenes must validate against the shared JSON Schema."""

import json
from pathlib import Path

import jsonschema
import pytest

ROOT = Path(__file__).resolve().parents[2]
SCHEMA = json.loads((ROOT / "schema" / "scene-v0.1.json").read_text())
SCENES = sorted((ROOT / "examples" / "scenes").glob("*.json"))


@pytest.mark.parametrize("path", SCENES, ids=lambda p: p.name)
def test_example_scene_validates(path):
    jsonschema.validate(json.loads(path.read_text()), SCHEMA)


def test_schema_rejects_missing_yaw():
    scene = json.loads((ROOT / "examples" / "scenes" / "tabletop.json").read_text())
    del scene["objects"][0]["yaw"]
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(scene, SCHEMA)
