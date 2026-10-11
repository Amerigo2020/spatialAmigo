# Release checklist (crates.io, PyPI, npm)

Nothing here has been run. Publishing is a manual decision by the maintainer.

## Before any release

1. Pick the version (SemVer, `0.x` while the API moves). Set it once in `[workspace.package] version` in `Cargo.toml`; `pyproject.toml` takes the version from Cargo through maturin. Check `CITATION.cff` (`version`, add `date-released`) and `version()` in the WASM crate.
2. Update the README (numbers, limitations) and `docs/decisions.md`.
3. CI green on `main`, including the MSRV job.
4. Commit, then tag: `git tag -a v0.1.0 -m "spatialAmigo 0.1.0"` and `git push origin v0.1.0`.

## crates.io (`spatialamigo`)

1. `cargo package -p spatialamigo --list` and check that `schema/` and `tests/fixtures/` are included (D11).
2. `cargo publish -p spatialamigo --dry-run`.
3. `cargo login` with a token scoped to publishing new crates, then `cargo publish -p spatialamigo`.
4. `spatialamigo-eval`, `spatialamigo-py` and `spatialamigo-wasm` already set `publish = false`.

## PyPI (`spatialamigo`)

1. Configure Trusted Publishing on pypi.org (pending publisher: owner `Amerigo2020`, repo `spatialAmigo`, workflow file of the release workflow, environment `pypi`). No API token is stored in the repository.
2. Add a release workflow triggered by `v*` tags: build wheels for Linux (manylinux), macOS (x86_64 and arm64) and Windows with `PyO3/maturin-action`, build the sdist with `maturin sdist`, upload with `pypa/gh-action-pypi-publish` using `id-token: write`.
3. Try it first against TestPyPI (separate trusted publisher), install from there into a fresh venv and run `pytest`.
4. The sdist was checked locally: it builds, installs in a fresh venv and passes the test suite.

## npm (WASM package, optional)

1. `wasm-pack build crates/spatialamigo-wasm --release --target bundler` (or `web`), check `package.json` name, license and repository fields.
2. `npm publish --dry-run` in `crates/spatialamigo-wasm/pkg`, then `npm publish --access public` with 2FA.

## After the release

* GitHub release from the tag with the changelog (manual).
* Add crates.io, PyPI and docs.rs badges to the README.
