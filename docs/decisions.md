# Design decisions

Decisions taken while building spatialAmigo autonomously. Each entry states the decision, the reason and what would change it.

## D1. Repository and git setup

* Work happens in a local git repository first. The session had no GitHub write access to `Amerigo2020/spatialAmigo` (the `add_repo` tool was not available and the GitHub API answered "GitHub access to this repository is not enabled for this session"). Pushing is retried at every milestone.
* Commits are authored as Amerigo Velletti with his GitHub noreply address so they count in his contribution graph. Every commit carries the `Co-Authored-By` trailer for Claude.
* Commit signing is disabled for this repository. The sandbox signs with a key that is not registered on Amerigo's account, which would show every commit as "Unverified" on GitHub.

## D2. Workspace layout

* `crates/spatialamigo` is the core: geometry, relations, scoring, query evaluation, trace, template parser. Dependencies are limited to `serde`, `serde_json` and `thiserror` so it compiles unchanged to WASM. `#![forbid(unsafe_code)]`.
* `crates/spatialamigo-eval` holds the synthetic benchmark (scene generator, ground truth oracles, runner). It is kept out of the core so the library stays small.
* `crates/spatialamigo-py` (PyO3, abi3 for Python 3.9+) and `crates/spatialamigo-wasm` (wasm-bindgen) are thin binding layers.
* The Python project (`pyproject.toml`) lives at the repository root so that `uv sync`, `uv run maturin develop` and `uv run pytest` work from the root without changing directories. Python sources are in `python/spatialamigo`.
* Licensing is `MIT OR Apache-2.0`, the Rust ecosystem default.

## D3. Scene format v0.1

* Implemented exactly as specified. The JSON Schema is strict (`additionalProperties: false`, `yaw` required, `front` and `viewpoint` optional) because the format is shared with motionAmigo and must only change with a version bump.
* The Rust loader validates what the schema cannot express on its own: unique ids, finite numbers, strictly positive sizes, `front` with unit length (tolerance 1e-3) and a viewpoint whose viewing direction has a horizontal component.
* Unknown JSON fields are ignored by the Rust reader (tolerant reader), the schema remains the strict contract. This keeps old binaries usable if motionAmigo adds optional fields in a later version.

## D4. Relation semantics

* Definitions follow the Sr3D generation rules from the ReferIt3D supplementary material (horizontal proximity, between, vertical proximity, support, allocentric) and make each hard threshold continuous. Full formulas in `docs/relations.md`.
* Horizontal distance is the gap between top-view footprints plus 0.1 times the center distance. Footprint gap matches how people judge "close to the table"; the center term breaks ties when footprints overlap.
* Closest and farthest are superlatives: their scores are normalized over all candidates of the target class with a soft min/max (`exp(-Δ/τ)`, τ = 0.15 m), so the winner always scores 1 and the runner-up's score shows the margin.
* `under` is the support relation (target holds the anchor). Non-touching "below" is `below`. This keeps the user-requested family split (support: on, under; vertical: above, below) while staying compatible with Sr3D, where "supporting" and "below/under without contact" are different relations.
* Directional relations use the four sections bounded by the anchor footprint diagonals (Sr3D allocentric sections) and a mild distance fall-off so the nearest object in a section wins.
* Egocentric frames use the line of sight from the viewpoint to each anchor rather than the global viewing direction. Default frame `auto` is allocentric when the anchor has a front, otherwise egocentric.
* Constraints combine with a product (AND). Anchor labels with several instances use the best anchor (existential reading).
* No dependency on a linear algebra crate: the core only needs 2D convex polygon routines (clipping, hull, distance) that are short, exact and keep the WASM build small.

## D5. Query model and parser

* A query is a target label, a list of constraints (AND) and an optional candidate id list. Anchors are referenced by label (string) or by id (`{"id": ...}`), which the Sr3D adapter needs because Sr3D gives anchor ids. Schema: `schema/query-v0.1.json`.
* Unknown JSON fields are rejected in queries (`deny_unknown_fields`). Queries come from LLMs, and a typo such as `constraint` instead of `constraints` must fail loudly instead of silently returning every candidate.
* The template parser is a phrase table with longest match plus vocabulary matching against the scene labels. It needs no grammar library and compiles to WASM. Labels may contain stop words ("trash can"), so labels are matched on raw tokens and the match that ends last wins (head noun).
* Frame markers such as "from my perspective" select the egocentric frame for all directional constraints in the sentence; "from the chair's point of view" selects allocentric.
* "near", "next to", "beside" and "by" map to `closest`. Sr3D only uses closest/farthest, and a superlative is the most useful reading for picking one object.
* The explanation warns when the best score is below 0.5 (weak match) or the margin is below 0.1 (ambiguous). The decision stays deterministic, but the caller (or a robot) can ask for clarification instead of acting.
* Minimum supported Rust version is 1.82 (`Option::is_none_or`).

## D6. Synthetic benchmark

* Samples are built constructively and then verified by a crisp, threshold-based oracle that re-implements the Sr3D rules separately from the soft scores, so the ground truth does not reuse the code under test.
* Own SplitMix64 generator instead of the `rand` crate: bit-for-bit reproducible across platforms and crate versions, no dependency.
* Ids are shuffled within each label because the resolver breaks exact ties by id; otherwise the target would often win ties by being `_1`.
* Clean accuracy is reported but framed as a consistency check. Three additional views make the benchmark informative: a relation-agnostic "nearest to the anchor" baseline, Gaussian pose noise on all objects, and held-out paraphrases written after the parser.
* `results/synthetic/report.md` and `report.json` are committed so the README numbers are traceable; the per-sample `samples.jsonl` is regenerated on demand and ignored by git.

## D7. Python bindings and LLM parser

* PyO3 0.29 with the `abi3-py39` feature: one wheel per platform covers every CPython from 3.9 on.
* Data crosses the boundary as JSON (dicts in, dicts out) instead of mirrored Python classes. The Rust types stay the single source of truth for validation, and results are trivially serializable for logging or a web UI.
* Exceptions form a hierarchy under `SpatialAmigoError(ValueError)`: `SceneError`, `QueryError`, `ParseError`, `ReferenceError`.
* The GIL is released (`py.detach`) during resolution.
* The LLM parser has no SDK dependency (plain `urllib`). Providers: OpenAI Chat Completions with `response_format` JSON schema in strict mode (also covers vLLM, Ollama and other compatible servers) and the Anthropic Messages API with a forced tool call whose `input_schema` is the query schema.
* The LLM schema is a strict-mode subset of `query-v0.1.json` (all fields required, `frame` explicit, labels as strings) and restricts `target` and `anchors` to an enum of the scene's labels. Output is validated by the Rust core; one retry with the validation error is allowed.
* No model ids are hard-coded as defaults. Model names change quickly and a silent default would age badly; the caller passes `model`.
* The LLM fixtures are hand-written in the providers' documented response formats because no API keys were available in this session. They are marked `"recorded": false`, and `scripts/record_llm_fixtures.py` replaces them with live responses.
