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
