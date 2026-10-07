"""Free text with an LLM, decision by the deterministic resolver.

    ANTHROPIC_API_KEY=... uv run python examples/python/llm_parser.py anthropic <model>
    OPENAI_API_KEY=...    uv run python examples/python/llm_parser.py openai <model>
    uv run python examples/python/llm_parser.py ollama llama3.1   # local, OpenAI-compatible

Without arguments it runs offline with a canned response to show the flow.
"""

import json
import sys
from pathlib import Path

import spatialamigo as sa
from spatialamigo.llm import Anthropic, LLMParser, OpenAICompatible

scene = sa.Scene.from_file(Path(__file__).resolve().parents[1] / "scenes" / "tabletop.json")
resolver = sa.Resolver(scene)
text = "grab the cup that sits left of my laptop"

if len(sys.argv) >= 3:
    kind, model = sys.argv[1], sys.argv[2]
    if kind == "anthropic":
        provider = Anthropic(model=model)
    elif kind == "ollama":
        provider = OpenAICompatible(model=model, base_url="http://localhost:11434/v1", api_key="")
    else:
        provider = OpenAICompatible(model=model)
else:
    canned = {"target": "mug", "constraints": [{"relation": "left", "anchors": ["laptop"], "frame": "auto"}]}
    provider = OpenAICompatible(
        model="offline-demo",
        api_key="",
        transport=lambda url, headers, payload: {"choices": [{"message": {"content": json.dumps(canned)}}]},
    )

parser = LLMParser(provider)
query = parser.parse(text, scene.labels)
print("LLM query:", json.dumps(query))
print(resolver.resolve(query).explanation)
