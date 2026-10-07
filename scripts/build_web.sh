#!/usr/bin/env bash
# Builds the static web demo into web/dist (WASM package, three.js, scenes).
#   scripts/build_web.sh            # release build with wasm-opt
#   WASM_NO_OPT=1 scripts/build_web.sh   # skip wasm-opt (e.g. offline)
# Serve with: python3 -m http.server -d web/dist 8000
set -euo pipefail
cd "$(dirname "$0")/.."

opt_flag=()
if [[ "${WASM_NO_OPT:-0}" == "1" ]]; then opt_flag=(--no-opt); fi
wasm-pack build crates/spatialamigo-wasm --release --target web --out-dir ../../web/pkg "${opt_flag[@]}"

(cd web && npm install --no-audit --no-fund --silent)

rm -rf web/dist
mkdir -p web/dist/vendor/three/addons/controls web/dist/scenes web/dist/pkg
cp web/index.html web/main.js web/style.css web/dist/
cp web/pkg/spatialamigo_wasm.js web/pkg/spatialamigo_wasm_bg.wasm web/dist/pkg/
cp web/node_modules/three/build/three.module.min.js web/dist/vendor/three/
cp web/node_modules/three/examples/jsm/controls/OrbitControls.js web/dist/vendor/three/addons/controls/
cp web/node_modules/three/LICENSE web/dist/vendor/three/LICENSE
cp examples/scenes/*.json web/dist/scenes/
touch web/dist/.nojekyll
echo "built web/dist ($(du -sh web/dist | cut -f1))"
