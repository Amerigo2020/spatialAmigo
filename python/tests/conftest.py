import json
from pathlib import Path

import pytest

import spatialamigo as sa

ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture
def tabletop_path() -> Path:
    return ROOT / "examples" / "scenes" / "tabletop.json"


@pytest.fixture
def tabletop(tabletop_path) -> dict:
    return json.loads(tabletop_path.read_text())


@pytest.fixture
def resolver(tabletop) -> sa.Resolver:
    return sa.Resolver(tabletop)
