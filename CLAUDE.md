# spatialAmigo, notes for agents

Deterministic, explainable resolution of spatial referring expressions in 3D scenes. Rust core, PyO3 bindings, WASM demo. Public repo, demo on GitHub Pages: https://amerigo2020.github.io/spatialAmigo/

## Commands
* Rust: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --exclude spatialamigo-py`
* Python: `uv sync`, `uv run maturin develop --release`, `uv run pytest`
* Synthetic benchmark (numbers in README): `cargo run --release -p spatialamigo-eval --bin synth-bench -- --seed 42 --per-task 400 --out results/synthetic`
* Web demo: `scripts/build_web.sh`, then serve `web/dist`

## Rules
* Decisions go to `docs/decisions.md` (D1 to D12). Release steps: `docs/release.md`. Nothing is published to crates.io, PyPI or npm.
* `crates/spatialamigo/schema/` and `tests/fixtures/tabletop.json` are copies of `schema/` and `examples/scenes/tabletop.json`. Edit the originals, copy them over; `tests/vendored_files.rs` fails on drift.
* MSRV 1.82 is checked in CI. Local stable may be older or newer; annotate generics instead of relying on new inference.
* No ScanNet or Scan2CAD data in the repo. Derived files (`eval/sr3d/out/`, `eval/sr3d/fronts*.json`) are gitignored.
* README numbers only from own runs. LLM fixtures: OpenAI recorded live (gpt-6-luna), Anthropic still hand-written.
* Remote is HTTPS: the local SSH key belongs to another GitHub account. Check `gh auth status` shows Amerigo2020.

## Open
* Sr3D evaluation needs ScanNet (terms of use) and sr3d.csv; then `eval/sr3d/scan2cad_to_fronts.py referit3d ...` and `eval/sr3d/run.py --fronts ...` (see eval/sr3d/README.md).
* Anthropic fixture: `uv run python scripts/record_llm_fixtures.py anthropic --model <id> --no-temperature`.
