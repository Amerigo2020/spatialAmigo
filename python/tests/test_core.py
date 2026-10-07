import json

import jsonschema
import pytest

import spatialamigo as sa


def test_scene_loading(tabletop, tabletop_path):
    s1 = sa.Scene(tabletop)
    s2 = sa.Scene(json.dumps(tabletop))
    s3 = sa.Scene.from_file(tabletop_path)
    assert len(s1) == len(s2) == len(s3) == 7
    assert s1.labels == ["book", "bowl", "chair", "laptop", "mug", "table"]
    assert s1.has_viewpoint
    assert s1.to_dict()["objects"][0]["id"] == "table_1"
    assert sa.Scene(s1.to_json()).ids == s1.ids


@pytest.mark.parametrize(
    "mutate",
    [
        lambda s: s.update(version="0.2"),
        lambda s: s["objects"][0].update(size=[0, 1, 1]),
        lambda s: s["objects"][1].update(id="table_1"),
        lambda s: s["objects"][0].pop("yaw"),
    ],
)
def test_invalid_scenes(tabletop, mutate):
    mutate(tabletop)
    with pytest.raises(sa.SceneError):
        sa.Scene(tabletop)


@pytest.mark.parametrize(
    ("text", "best"),
    [
        ("the mug closest to the laptop", "mug_2"),
        ("the mug farthest from the laptop", "mug_1"),
        ("the mug left of the laptop", "mug_2"),
        ("the mug to the right of the laptop", "mug_1"),
        ("the mug closest to the bowl", "mug_1"),
        ("the mug to the left of the chair", "mug_1"),
        ("the mug to the left of the chair from my perspective", "mug_2"),
        ("the book on the table", "book_1"),
    ],
)
def test_resolve_text(resolver, text, best):
    res = resolver.resolve(text)
    assert res.best == best, res.explanation
    assert res.candidates[0]["id"] == best
    assert res.scores == sorted(res.scores, key=lambda p: -p[1])


def test_resolve_structured_and_trace(resolver):
    q = {"target": "mug", "constraints": [{"relation": "left", "anchors": ["chair"], "frame": "egocentric"}]}
    res = resolver.resolve(q)
    assert res.best == "mug_2"
    trace = res.candidates[0]["trace"][0]
    assert trace["frame"] == "egocentric"
    assert trace["family"] == "egocentric"
    assert trace["anchors"] == ["chair_1"]
    names = [f["name"] for f in trace["features"]]
    assert "sector" in names and "distance_falloff" in names
    assert res.query == q
    d = res.to_dict()
    assert d["best"] == "mug_2" and json.loads(res.to_json())["margin"] == pytest.approx(res.margin)


def test_errors(resolver):
    with pytest.raises(sa.ParseError):
        resolver.resolve("the cat on the table")
    with pytest.raises(sa.ReferenceError):
        resolver.resolve({"target": "cat"})
    with pytest.raises(sa.QueryError):
        resolver.resolve({"target": "mug", "constraints": [{"relation": "between", "anchors": ["book"]}]})
    with pytest.raises(sa.QueryError):
        resolver.resolve({"target": "mug", "constraint": []})
    assert issubclass(sa.ParseError, sa.SpatialAmigoError)
    assert issubclass(sa.SpatialAmigoError, ValueError)


def test_resolve_many(resolver):
    out = resolver.resolve_many(["the mug left of the laptop", "nonsense", {"target": "book"}])
    assert out[0].best == "mug_2"
    assert isinstance(out[1], sa.ParseError)
    assert out[2].best == "book_1"


def test_parse_and_schema():
    q = sa.parse("the mug between the laptop and the book, on the table", ["mug", "laptop", "book", "table"])
    assert q == {
        "target": "mug",
        "constraints": [
            {"relation": "between", "anchors": ["laptop", "book"]},
            {"relation": "on", "anchors": ["table"]},
        ],
    }
    jsonschema.validate(q, sa.QUERY_SCHEMA)
    assert sa.SCENE_SCHEMA["properties"]["version"]["const"] == "0.1"


def test_validate_query_canonicalizes():
    q = sa.validate_query({"target": "mug", "constraints": [{"relation": "on", "anchors": [{"label": "table"}]}]})
    assert q["constraints"][0]["anchors"] == ["table"]
    assert sa.validate_query({"target": "mug", "constraints": [{"relation": "on", "anchors": [{"id": "t"}]}]})[
        "constraints"
    ][0]["anchors"] == [{"id": "t"}]


def test_params_override(tabletop):
    defaults = sa.default_params()
    assert defaults["proximity_tau"] == pytest.approx(0.15)
    sharp = sa.Resolver(tabletop, params={"proximity_tau": 0.01})
    assert sharp.params["proximity_tau"] == pytest.approx(0.01)
    soft = sa.Resolver(tabletop)
    s_sharp = dict(sharp.resolve("the mug closest to the laptop").scores)
    s_soft = dict(soft.resolve("the mug closest to the laptop").scores)
    assert s_sharp["mug_1"] < s_soft["mug_1"]
    with pytest.raises(sa.QueryError):
        sa.Resolver(tabletop, params={"proximity_tau": "x"})


def test_module_helper(tabletop):
    assert sa.resolve(tabletop, "the mug closest to the laptop").best == "mug_2"
    assert sa.__version__ == "0.1.0"
