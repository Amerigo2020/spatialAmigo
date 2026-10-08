"""Optional LLM parser: free text to a structured spatialAmigo query.

The language model only *translates*. It fills a JSON schema (structured
output) whose label fields are restricted to the labels present in the scene,
the result is validated by the Rust core, and the decision is made by the
deterministic resolver. This follows the TRACE principle: the LLM proposes a
query, deterministic tools decide and explain.

Two provider families are supported without any SDK dependency:

* :class:`OpenAICompatible` for the OpenAI Chat Completions API and every
  server that implements it (vLLM, Ollama, LM Studio, Together, Groq, ...),
  using ``response_format={"type": "json_schema", ...}``.
* :class:`Anthropic` for the Claude Messages API, using a forced tool call whose
  ``input_schema`` is the query schema.

Everything is testable without an API key: pass a ``transport`` callable that
receives ``(url, headers, payload)`` and returns the decoded JSON response.
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable, Mapping, Optional, Sequence, Union

from . import FRAMES, RELATIONS, QueryError, Resolution, Resolver, validate_query

Transport = Callable[[str, Mapping[str, str], Mapping[str, Any]], Mapping[str, Any]]
"""``(url, headers, payload) -> decoded JSON response``."""


class LLMError(RuntimeError):
    """The provider failed or returned something that is not a valid query."""


SYSTEM_PROMPT = """You translate a referring expression about objects in a 3D scene into a structured query.
You never decide which object is meant. You only describe what the sentence asks for.

Fields:
- target: the class label of the object the user refers to. Use exactly one of the allowed labels.
- constraints: every spatial condition in the sentence (they are combined with AND). Each has
  - relation: one of
    closest, farthest            horizontal proximity (also: near, next to, by, far from)
    between                      needs exactly two anchors
    above, below                 vertically separated, not touching
    on                           target rests on / is supported by the anchor
    under                        target is directly under and supports the anchor ("the table with the laptop on it" = table under laptop)
    left, right, front, behind   directional
  - anchors: the reference object labels (exactly one, two for between). Use only allowed labels.
  - frame: for left/right/front/behind:
    "egocentric" if the sentence speaks from the speaker's or camera's view ("from my perspective", "as I see it", "on my left"),
    "allocentric" if it uses the anchor's own orientation ("from the chair's point of view", "on the sofa's left", "facing the TV" means front of the TV),
    "auto" otherwise. Use "auto" for all non-directional relations.
Ignore colors, sizes and other attributes, they are not supported. If the sentence names an object with a word that is not an allowed label, use the allowed label for the same kind of object
(a synonym or near-synonym, e.g. couch -> sofa, trash bin -> trash can), judged by meaning, not by position in the scene.

Examples:
"the mug closest to the laptop" -> {"target":"mug","constraints":[{"relation":"closest","anchors":["laptop"],"frame":"auto"}]}
"the table that the book is on" -> {"target":"table","constraints":[{"relation":"under","anchors":["book"],"frame":"auto"}]}
"the plant on the bed's left" -> {"target":"plant","constraints":[{"relation":"left","anchors":["bed"],"frame":"allocentric"}]}
"the box between the sofa and the tv, from my perspective" -> {"target":"box","constraints":[{"relation":"between","anchors":["sofa","tv"],"frame":"auto"}]}"""


def query_schema(labels: Optional[Sequence[str]] = None) -> dict[str, Any]:
    """JSON schema for structured output.

    It is the strict subset of ``schema/query-v0.1.json`` that providers accept
    in strict mode (all fields required, no ``oneOf``). With ``labels`` the
    target and anchors are restricted to an enum, so the model cannot invent
    objects that are not in the scene.
    """
    label: dict[str, Any] = {"type": "string"}
    if labels:
        label = {"type": "string", "enum": sorted(set(labels))}
    return {
        "type": "object",
        "additionalProperties": False,
        "required": ["target", "constraints"],
        "properties": {
            "target": label,
            "constraints": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": False,
                    "required": ["relation", "anchors", "frame"],
                    "properties": {
                        "relation": {"type": "string", "enum": list(RELATIONS)},
                        "anchors": {"type": "array", "items": label},
                        "frame": {"type": "string", "enum": list(FRAMES)},
                    },
                },
            },
        },
    }


def user_prompt(text: str, labels: Optional[Sequence[str]]) -> str:
    allowed = ", ".join(sorted(set(labels))) if labels else "(any)"
    return f"Allowed labels: {allowed}\nSentence: {text}"


def http_transport(timeout: float = 60.0) -> Transport:
    """Default transport based on :mod:`urllib` (no third-party dependency)."""

    def send(url: str, headers: Mapping[str, str], payload: Mapping[str, Any]) -> Mapping[str, Any]:
        req = urllib.request.Request(url, data=json.dumps(payload).encode(), headers=dict(headers), method="POST")
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310 (URL is configured by the caller)
                return json.loads(resp.read().decode())
        except urllib.error.HTTPError as e:
            body = e.read().decode(errors="replace")[:500]
            raise LLMError(f"HTTP {e.code} from {url}: {body}") from e
        except urllib.error.URLError as e:
            raise LLMError(f"cannot reach {url}: {e.reason}") from e

    return send


class Provider:
    """Base class: turns (system, user, schema) into a JSON object."""

    name = "provider"

    def complete(self, system: str, user: str, schema: Mapping[str, Any]) -> dict[str, Any]:
        raise NotImplementedError


@dataclass
class OpenAICompatible(Provider):
    """OpenAI Chat Completions with ``response_format`` JSON schema (strict).

    ``base_url`` can point at any compatible server, e.g.
    ``http://localhost:11434/v1`` for Ollama or ``http://localhost:8000/v1`` for vLLM.
    """

    model: str
    api_key: Optional[str] = None
    base_url: str = "https://api.openai.com/v1"
    temperature: Optional[float] = 0.0  # None omits the field (reasoning models reject it)
    strict: bool = True
    transport: Optional[Transport] = None
    extra_headers: dict[str, str] = field(default_factory=dict)
    name = "openai-compatible"

    def complete(self, system: str, user: str, schema: Mapping[str, Any]) -> dict[str, Any]:
        key = self.api_key if self.api_key is not None else os.environ.get("OPENAI_API_KEY", "")
        headers = {"Content-Type": "application/json", **self.extra_headers}
        if key:
            headers["Authorization"] = f"Bearer {key}"
        payload = {
            "model": self.model,
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "spatial_query", "strict": self.strict, "schema": schema},
            },
        }
        if self.temperature is not None:
            payload["temperature"] = self.temperature
        resp = (self.transport or http_transport())(f"{self.base_url.rstrip('/')}/chat/completions", headers, payload)
        try:
            msg = resp["choices"][0]["message"]
        except (KeyError, IndexError, TypeError) as e:
            raise LLMError(f"unexpected response shape: {str(resp)[:300]}") from e
        if msg.get("refusal"):
            raise LLMError(f"model refused: {msg['refusal']}")
        content = msg.get("content")
        if not isinstance(content, str):
            raise LLMError("response has no text content")
        try:
            return json.loads(content)
        except json.JSONDecodeError as e:
            raise LLMError(f"model returned invalid JSON: {content[:200]}") from e


@dataclass
class Anthropic(Provider):
    """Claude Messages API with a forced tool call carrying the query schema."""

    model: str
    api_key: Optional[str] = None
    base_url: str = "https://api.anthropic.com"
    max_tokens: int = 1024
    temperature: Optional[float] = 0.0  # None omits the field (reasoning models reject it)
    anthropic_version: str = "2023-06-01"
    transport: Optional[Transport] = None
    name = "anthropic"

    TOOL = "emit_query"

    def complete(self, system: str, user: str, schema: Mapping[str, Any]) -> dict[str, Any]:
        key = self.api_key if self.api_key is not None else os.environ.get("ANTHROPIC_API_KEY", "")
        headers = {
            "Content-Type": "application/json",
            "x-api-key": key,
            "anthropic-version": self.anthropic_version,
        }
        payload = {
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": [{"role": "user", "content": user}],
            "tools": [
                {
                    "name": self.TOOL,
                    "description": "Emit the structured spatial query for the sentence.",
                    "input_schema": schema,
                }
            ],
            "tool_choice": {"type": "tool", "name": self.TOOL},
        }
        if self.temperature is not None:
            payload["temperature"] = self.temperature
        resp = (self.transport or http_transport())(f"{self.base_url.rstrip('/')}/v1/messages", headers, payload)
        for block in resp.get("content", []) if isinstance(resp, Mapping) else []:
            if block.get("type") == "tool_use" and block.get("name") == self.TOOL:
                if not isinstance(block.get("input"), dict):
                    raise LLMError("tool input is not an object")
                return block["input"]
        raise LLMError(f"no {self.TOOL} tool call in response: {str(resp)[:300]}")


@dataclass
class LLMParser:
    """Free text to structured query with a language model, validated by the Rust core.

    >>> parser = LLMParser(Anthropic(model="claude-..."))  # doctest: +SKIP
    >>> parser.resolve(resolver, "grab the cup that sits left of my laptop")  # doctest: +SKIP
    """

    provider: Provider
    max_retries: int = 1
    restrict_labels: bool = True
    system_prompt: str = SYSTEM_PROMPT

    def parse(self, text: str, labels: Optional[Sequence[str]] = None) -> dict[str, Any]:
        """Returns a validated query dict. Raises :class:`LLMError` if the model
        does not produce a valid query after ``max_retries`` corrections."""
        schema = query_schema(labels if self.restrict_labels else None)
        user = user_prompt(text, labels)
        last_error = ""
        for _ in range(self.max_retries + 1):
            prompt = user if not last_error else f"{user}\nYour previous answer was invalid: {last_error}. Fix it."
            try:
                raw = self.provider.complete(self.system_prompt, prompt, schema)
                query = validate_query(_normalize(raw))
                _check_labels(query, labels)
                return query
            except (QueryError, LLMError, ValueError) as e:
                if isinstance(e, LLMError) and "HTTP" in str(e):
                    raise
                last_error = str(e)
        raise LLMError(f"no valid query from {self.provider.name}: {last_error}")

    def resolve(self, resolver: Resolver, text: str) -> Resolution:
        """Parse with the LLM, then resolve deterministically."""
        return resolver.resolve(self.parse(text, resolver.scene.labels))


def _normalize(raw: Union[Mapping[str, Any], str]) -> dict[str, Any]:
    if isinstance(raw, str):
        raw = json.loads(raw)
    if not isinstance(raw, Mapping):
        raise LLMError("model output is not a JSON object")
    out: dict[str, Any] = {"target": raw.get("target"), "constraints": []}
    for c in raw.get("constraints") or []:
        c = dict(c)
        if c.get("frame") in (None, "auto"):
            c.pop("frame", None)
        out["constraints"].append(c)
    return out


def _check_labels(query: Mapping[str, Any], labels: Optional[Sequence[str]]) -> None:
    if not labels:
        return
    allowed = {label.lower() for label in labels}
    used = [query["target"]] + [a for c in query["constraints"] for a in c["anchors"] if isinstance(a, str)]
    unknown = [u for u in used if u.lower() not in allowed]
    if unknown:
        raise QueryError(f"labels not in the scene: {unknown}")


__all__ = [
    "Anthropic",
    "LLMError",
    "LLMParser",
    "OpenAICompatible",
    "Provider",
    "SYSTEM_PROMPT",
    "Transport",
    "http_transport",
    "query_schema",
]
