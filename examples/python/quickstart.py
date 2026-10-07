"""Quickstart: resolve referring expressions in the tabletop scene.

    uv run python examples/python/quickstart.py
"""

from pathlib import Path

import spatialamigo as sa

scene = sa.Scene.from_file(Path(__file__).resolve().parents[1] / "scenes" / "tabletop.json")
resolver = sa.Resolver(scene)

for text in [
    "the mug closest to the laptop",
    "the mug left of the laptop",
    "the mug to the left of the chair from my perspective",
]:
    res = resolver.resolve(text)
    print(f"{text!r} -> {res.best} (margin {res.margin:.2f})")

# Structured query, e.g. produced by an LLM or a planner:
res = resolver.resolve({"target": "mug", "constraints": [{"relation": "right", "anchors": ["laptop"]}]})
print(res.explanation)
