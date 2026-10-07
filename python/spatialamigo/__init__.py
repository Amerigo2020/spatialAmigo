"""spatialAmigo: deterministic, explainable resolution of spatial referring expressions.

>>> import spatialamigo as sa
>>> r = sa.Resolver(sa.Scene.from_file("examples/scenes/tabletop.json"))  # doctest: +SKIP
>>> r.resolve("the mug left of the laptop").best  # doctest: +SKIP
'mug_2'

The heavy lifting happens in Rust (``spatialamigo._core``). The optional
:mod:`spatialamigo.llm` module translates free text into the same structured
query with a language model; the decision stays deterministic.
"""

from __future__ import annotations

from typing import Any, Mapping, Union

from ._core import (
    QUERY_SCHEMA,
    SCENE_SCHEMA,
    ParseError,
    QueryError,
    ReferenceError,
    Resolution,
    Resolver,
    Scene,
    SceneError,
    SpatialAmigoError,
    __version__,
    default_params,
    parse,
    validate_query,
)

RELATIONS = (
    "closest",
    "farthest",
    "between",
    "above",
    "below",
    "on",
    "under",
    "left",
    "right",
    "front",
    "behind",
)
"""All relation names, in the order of ``schema/query-v0.1.json``."""

FRAMES = ("auto", "allocentric", "egocentric")
"""Reference frames for left/right/front/behind."""


def resolve(scene: Union[Scene, Mapping[str, Any], str], query: Union[str, Mapping[str, Any]]) -> Resolution:
    """One-shot helper: build a resolver for ``scene`` and resolve ``query``.

    Prefer :class:`Resolver` when resolving several queries in the same scene.
    """
    return Resolver(scene).resolve(query)


__all__ = [
    "FRAMES",
    "QUERY_SCHEMA",
    "RELATIONS",
    "SCENE_SCHEMA",
    "ParseError",
    "QueryError",
    "ReferenceError",
    "Resolution",
    "Resolver",
    "Scene",
    "SceneError",
    "SpatialAmigoError",
    "__version__",
    "default_params",
    "parse",
    "resolve",
    "validate_query",
]
