"""LLM parser tests. No network and no API key: providers get a fake transport."""

import json
from pathlib import Path

import pytest

import spatialamigo as sa
from spatialamigo.llm import Anthropic, LLMError, LLMParser, OpenAICompatible, query_schema

FIXTURES = Path(__file__).parent / "fixtures" / "llm"
LABELS = ["book", "bowl", "chair", "laptop", "mug", "table"]


class Recorder:
    """Fake transport: returns queued responses and records requests."""

    def __init__(self, *responses):
        self.responses = list(responses)
        self.requests = []

    def __call__(self, url, headers, payload):
        self.requests.append((url, dict(headers), json.loads(json.dumps(payload))))
        r = self.responses.pop(0)
        if isinstance(r, Exception):
            raise r
        return r


def openai_response(content):
    return {"choices": [{"message": {"role": "assistant", "content": content, "refusal": None}}]}


def anthropic_response(query):
    return {"content": [{"type": "tool_use", "id": "t", "name": "emit_query", "input": query}]}


def fixture_cases(provider):
    data = json.loads((FIXTURES / f"{provider}.json").read_text())
    return [pytest.param(provider, c, id=f"{provider}:{c['sentence'][:40]}") for c in data["cases"]]


class FixtureTransport:
    def __init__(self, case):
        self.case = case

    def __call__(self, url, headers, payload):
        text = json.dumps(payload)
        assert self.case["sentence"] in text
        return self.case["response"]


@pytest.mark.parametrize(("provider", "case"), fixture_cases("openai") + fixture_cases("anthropic"))
def test_fixtures_end_to_end(resolver, provider, case):
    transport = FixtureTransport(case)
    p = OpenAICompatible(model="m", api_key="k", transport=transport) if provider == "openai" else Anthropic(
        model="m", api_key="k", transport=transport
    )
    parser = LLMParser(p)
    query = parser.parse(case["sentence"], case["labels"])
    expected = sa.validate_query({**case["expected_query"]})
    # "auto" frames are dropped during normalization
    assert query == sa.validate_query(
        {
            "target": expected["target"],
            "constraints": [
                {k: v for k, v in c.items() if not (k == "frame" and v == "auto")} for c in case["expected_query"]["constraints"]
            ],
        }
    )
    res = parser.resolve(resolver, case["sentence"])
    assert res.best == case["expected_best"], res.explanation


def test_template_parser_fails_where_llm_fixtures_help(resolver):
    # Documents the motivation: these sentences are outside the template grammar.
    for s in ["the table that the book is on", "grab the cup that sits left of my laptop"]:
        with pytest.raises(sa.ParseError):
            resolver.resolve(s)


def test_openai_request_shape():
    t = Recorder(openai_response(json.dumps({"target": "mug", "constraints": []})))
    OpenAICompatible(model="gpt-x", api_key="sk-test", base_url="http://localhost:8000/v1/", transport=t).complete(
        "sys", "user", query_schema(LABELS)
    )
    url, headers, payload = t.requests[0]
    assert url == "http://localhost:8000/v1/chat/completions"
    assert headers["Authorization"] == "Bearer sk-test"
    rf = payload["response_format"]
    assert rf["type"] == "json_schema" and rf["json_schema"]["strict"] is True
    schema = rf["json_schema"]["schema"]
    assert schema["properties"]["target"]["enum"] == LABELS
    assert schema["properties"]["constraints"]["items"]["required"] == ["relation", "anchors", "frame"]
    assert payload["messages"][0]["role"] == "system"


def test_anthropic_request_shape():
    t = Recorder(anthropic_response({"target": "mug", "constraints": []}))
    Anthropic(model="claude-x", api_key="ak-test", transport=t).complete("sys", "user", query_schema(LABELS))
    url, headers, payload = t.requests[0]
    assert url == "https://api.anthropic.com/v1/messages"
    assert headers["x-api-key"] == "ak-test"
    assert headers["anthropic-version"] == "2023-06-01"
    assert payload["tool_choice"] == {"type": "tool", "name": "emit_query"}
    assert payload["tools"][0]["input_schema"]["properties"]["target"]["enum"] == LABELS
    assert payload["system"] == "sys"


def test_retry_after_invalid_output():
    bad = {"target": "mug", "constraints": [{"relation": "near", "anchors": ["laptop"], "frame": "auto"}]}
    good = {"target": "mug", "constraints": [{"relation": "closest", "anchors": ["laptop"], "frame": "auto"}]}
    t = Recorder(anthropic_response(bad), anthropic_response(good))
    q = LLMParser(Anthropic(model="m", api_key="k", transport=t)).parse("the mug near the laptop", LABELS)
    assert q["constraints"][0]["relation"] == "closest"
    assert len(t.requests) == 2
    assert "previous answer was invalid" in t.requests[1][2]["messages"][0]["content"]


def test_gives_up_after_retries():
    t = Recorder(openai_response("not json"), openai_response("{}"))
    with pytest.raises(LLMError, match="no valid query"):
        LLMParser(OpenAICompatible(model="m", api_key="k", transport=t), max_retries=1).parse("x", LABELS)


def test_rejects_hallucinated_labels():
    halluc = {"target": "cup", "constraints": []}
    t = Recorder(openai_response(json.dumps(halluc)), openai_response(json.dumps(halluc)))
    with pytest.raises(LLMError, match="not in the scene"):
        LLMParser(OpenAICompatible(model="m", api_key="k", transport=t)).parse("the cup", LABELS)


def test_refusal_and_http_errors_are_reported():
    refusal = {"choices": [{"message": {"role": "assistant", "content": None, "refusal": "no"}}]}
    t = Recorder(refusal, refusal)
    with pytest.raises(LLMError, match="refused"):
        LLMParser(OpenAICompatible(model="m", api_key="k", transport=t)).parse("x", LABELS)
    t = Recorder(LLMError("HTTP 401 from url: bad key"))
    with pytest.raises(LLMError, match="401"):
        LLMParser(Anthropic(model="m", api_key="k", transport=t)).parse("x", LABELS)
    assert len(t.requests) == 1


def test_schema_without_labels_is_open():
    s = query_schema()
    assert "enum" not in s["properties"]["target"]
    assert s["properties"]["constraints"]["items"]["properties"]["relation"]["enum"] == list(sa.RELATIONS)
