"""Re-record the LLM parser fixtures against real providers.

Usage (needs API keys, makes paid API calls):

    OPENAI_API_KEY=... uv run python scripts/record_llm_fixtures.py openai --model <model>
    ANTHROPIC_API_KEY=... uv run python scripts/record_llm_fixtures.py anthropic --model <model>

The script keeps the sentences and expectations of the existing fixture file,
replaces every ``response`` with the raw provider response, and sets
``recorded`` to true. API keys are never written to disk. Review the diff
before committing: a changed ``expected_best`` mismatch shows up as a failing
test, which is the point.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from spatialamigo.llm import SYSTEM_PROMPT, Anthropic, OpenAICompatible, http_transport, query_schema, user_prompt

FIXTURES = Path(__file__).resolve().parents[1] / "python" / "tests" / "fixtures" / "llm"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("provider", choices=["openai", "anthropic"])
    ap.add_argument("--model", required=True)
    ap.add_argument("--base-url", default=None)
    ap.add_argument("--no-temperature", action="store_true", help="omit temperature (reasoning models reject 0)")
    args = ap.parse_args()

    path = FIXTURES / f"{args.provider}.json"
    data = json.loads(path.read_text())
    captured = []

    def transport(url, headers, payload):
        resp = http_transport()(url, headers, payload)
        captured.append(resp)
        return resp

    kwargs = {"model": args.model, "transport": transport}
    if args.no_temperature:
        kwargs["temperature"] = None
    if args.base_url:
        kwargs["base_url"] = args.base_url
    provider = OpenAICompatible(**kwargs) if args.provider == "openai" else Anthropic(**kwargs)
    for case in data["cases"]:
        captured.clear()
        out = provider.complete(SYSTEM_PROMPT, user_prompt(case["sentence"], case["labels"]), query_schema(case["labels"]))
        case["response"] = captured[-1]
        print(f"{case['sentence']!r} -> {json.dumps(out)}")
    data["recorded"] = True
    temp = "" if args.no_temperature else ", temperature 0"
    data["_note"] = f"Recorded from {args.provider} model {args.model}{temp} with scripts/record_llm_fixtures.py."
    path.write_text(json.dumps(data, indent=2) + "\n")
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
