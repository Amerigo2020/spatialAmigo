// spatialAmigo web demo: three.js scene + Rust resolver compiled to WebAssembly.
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import init, { WasmResolver, version } from "./pkg/spatialamigo_wasm.js";

const SCENES = {
  "Living room": {
    url: "scenes/living_room.json",
    examples: [
      "the armchair to the left of the sofa",
      "the plant to the right of the tv",
      "the plant to the left of the tv from my perspective",
      "the mug closest to the bowl",
      "the mug on the side table",
      "the picture above the sofa",
      "the book on the cabinet",
      "the plant closest to the armchair",
    ],
  },
  Tabletop: {
    url: "scenes/tabletop.json",
    examples: [
      "the mug left of the laptop",
      "the mug closest to the laptop",
      "the mug to the left of the chair",
      "the mug to the left of the chair from my perspective",
      "the mug on the table and to the right of the laptop",
      "the book on the table",
    ],
  },
};

const COLORS = {
  idle: 0x9aa3ad,
  cand: 0xf59e0b,
  best: 0x0e9f6e,
  anchor: 0x2f6fde,
  view: 0x8b5cf6,
};

const $ = (id) => document.getElementById(id);
const canvas = $("canvas");
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
const scene3 = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(45, 1, 0.05, 100);
camera.up.set(0, 0, 1);
const controls = new OrbitControls(camera, canvas);
controls.enableDamping = false;
controls.addEventListener("change", render);

const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
scene3.background = new THREE.Color(dark ? 0x0f1419 : 0xf6f7f9);
scene3.add(new THREE.HemisphereLight(0xffffff, dark ? 0x202830 : 0xb0b8c0, 1.6));
const sun = new THREE.DirectionalLight(0xffffff, 1.4);
sun.position.set(3, -4, 6);
scene3.add(sun);

let world = new THREE.Group();
scene3.add(world);
let state = { name: null, scene: null, resolver: null, objects: new Map(), result: null, selected: null };

function render() {
  renderer.render(scene3, camera);
}

function resize() {
  const r = canvas.parentElement.getBoundingClientRect();
  renderer.setSize(r.width, r.height, false);
  camera.aspect = r.width / Math.max(r.height, 1);
  camera.updateProjectionMatrix();
  render();
}
window.addEventListener("resize", resize);

function labelSprite(text) {
  const c = document.createElement("canvas");
  const ctx = c.getContext("2d");
  const font = "600 44px system-ui, sans-serif";
  ctx.font = font;
  const w = Math.ceil(ctx.measureText(text).width) + 28;
  c.width = w;
  c.height = 64;
  ctx.font = font;
  ctx.fillStyle = dark ? "rgba(22,28,35,0.85)" : "rgba(255,255,255,0.88)";
  ctx.beginPath();
  ctx.roundRect(0, 4, w, 56, 14);
  ctx.fill();
  ctx.fillStyle = dark ? "#e6eaee" : "#17202a";
  ctx.textBaseline = "middle";
  ctx.fillText(text, 14, 34);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: tex, depthTest: false, transparent: true }));
  const h = 0.13;
  s.scale.set((h * w) / 64, h, 1);
  s.renderOrder = 10;
  return s;
}

function buildWorld(scene) {
  scene3.remove(world);
  world = new THREE.Group();
  state.objects.clear();
  // floor grid in the XY plane
  const grid = new THREE.GridHelper(8, 32, dark ? 0x33404c : 0xc9d0d6, dark ? 0x222b33 : 0xe1e5e9);
  grid.rotation.x = Math.PI / 2;
  world.add(grid);
  for (const o of scene.objects) {
    const g = new THREE.Group();
    g.position.set(...o.center);
    g.rotation.z = o.yaw;
    const geo = new THREE.BoxGeometry(...o.size);
    const mat = new THREE.MeshStandardMaterial({ color: COLORS.idle, transparent: true, opacity: 0.18, roughness: 0.8, depthWrite: false });
    const mesh = new THREE.Mesh(geo, mat);
    const edges = new THREE.LineSegments(new THREE.EdgesGeometry(geo), new THREE.LineBasicMaterial({ color: COLORS.idle }));
    g.add(mesh, edges);
    if (o.front) {
      const len = Math.max(o.size[0], o.size[1]) * 0.55 + 0.1;
      const dir = new THREE.Vector3(...o.front).normalize();
      const arrow = new THREE.ArrowHelper(dir, new THREE.Vector3(0, 0, 0), len, 0x6b7785, 0.08, 0.05);
      g.add(arrow);
    }
    const label = labelSprite(o.id);
    label.position.set(o.center[0], o.center[1], o.center[2] + o.size[2] / 2 + 0.07);
    world.add(g, label);
    state.objects.set(o.id, { group: g, mesh, edges, label, obj: o });
  }
  if (scene.viewpoint) {
    const { position: p, look_at: l } = scene.viewpoint;
    const from = new THREE.Vector3(...p);
    const to = new THREE.Vector3(...l);
    const dir = to.clone().sub(from).normalize();
    const cone = new THREE.Mesh(new THREE.ConeGeometry(0.09, 0.22, 16), new THREE.MeshStandardMaterial({ color: COLORS.view }));
    cone.position.copy(from);
    cone.quaternion.setFromUnitVectors(new THREE.Vector3(0, -1, 0), dir);
    const line = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints([from, from.clone().add(dir.multiplyScalar(0.8))]),
      new THREE.LineDashedMaterial({ color: COLORS.view, dashSize: 0.06, gapSize: 0.04 }),
    );
    line.computeLineDistances();
    const lab = labelSprite("viewpoint");
    lab.position.copy(from).add(new THREE.Vector3(0, 0, 0.2));
    world.add(cone, line, lab);
  }
  scene3.add(world);
  // camera: behind and above the viewpoint, looking at its target
  const box = new THREE.Box3().setFromObject(world);
  const center = scene.viewpoint ? new THREE.Vector3(...scene.viewpoint.look_at) : box.getCenter(new THREE.Vector3());
  const eye = scene.viewpoint ? new THREE.Vector3(...scene.viewpoint.position) : center.clone().add(new THREE.Vector3(-3, -3, 2.5));
  const dirv = eye.clone().sub(center);
  dirv.z = Math.max(dirv.z, 0.9 * Math.hypot(dirv.x, dirv.y));
  const size = box.getSize(new THREE.Vector3()).length();
  camera.position.copy(center).add(dirv.normalize().multiplyScalar(Math.max(size * 0.78, 1.7)));
  controls.target.copy(center);
  controls.update();
  render();
}

function paint(result, selectedId) {
  const cands = new Map((result?.candidates ?? []).map((c) => [c.id, c]));
  const anchors = new Set();
  const sel = result?.candidates.find((c) => c.id === selectedId) ?? result?.candidates[0];
  for (const t of sel?.trace ?? []) for (const a of t.anchors) anchors.add(a);
  for (const [id, v] of state.objects) {
    let color = COLORS.idle;
    let opacity = 0.14;
    if (anchors.has(id)) {
      color = COLORS.anchor;
      opacity = 0.45;
    }
    if (cands.has(id)) {
      const c = cands.get(id);
      const isBest = result.best === id;
      color = isBest ? COLORS.best : COLORS.cand;
      opacity = isBest ? 0.85 : 0.12 + 0.6 * c.score;
    }
    if (selectedId && id === selectedId && result.best !== id) opacity = Math.max(opacity, 0.6);
    v.mesh.material.color.setHex(color);
    v.mesh.material.opacity = opacity;
    v.edges.material.color.setHex(color);
  }
  render();
}

const fmt = (x) => (Math.abs(x) >= 100 ? x.toFixed(0) : x.toFixed(3));

function showTrace(result, id) {
  const c = result.candidates.find((x) => x.id === id);
  $("trace-of").textContent = c ? `for ${c.id} (score ${c.score.toFixed(3)})` : "";
  $("trace").innerHTML = "";
  for (const t of c?.trace ?? []) {
    const div = document.createElement("div");
    div.className = "c";
    const frame = t.frame ? ` [${t.frame}]` : "";
    div.innerHTML = `<div class="h"><span>${t.relation}(${t.anchors.join(", ")})${frame}</span><span>${t.score.toFixed(3)}</span></div>`;
    if (t.note) div.innerHTML += `<div class="note">${t.note}</div>`;
    const dl = document.createElement("dl");
    for (const f of t.features) dl.innerHTML += `<dt>${f.name}</dt><dd>${fmt(f.value)}</dd>`;
    div.appendChild(dl);
    $("trace").appendChild(div);
  }
  [...$("candidates").querySelectorAll("tr")].forEach((tr) => tr.classList.toggle("sel", tr.dataset.id === id));
  paint(result, id);
}

function showResult(result) {
  state.result = result;
  const tbody = $("candidates").querySelector("tbody");
  tbody.innerHTML = "";
  for (const c of result.candidates) {
    const tr = document.createElement("tr");
    tr.dataset.id = c.id;
    tr.innerHTML = `<td class="rank">#${c.rank}</td><td>${c.id}</td><td class="score"><span class="bar"><i style="width:${(100 * c.score).toFixed(1)}%"></i></span><span class="num">${c.score.toFixed(3)}</span></td>`;
    tr.addEventListener("click", () => showTrace(result, c.id));
    tbody.appendChild(tr);
  }
  const warnings = result.explanation.split("\n").filter((l) => l.startsWith("warning:"));
  $("status").innerHTML = result.best
    ? `Selected <b>${result.best}</b> · score ${result.candidates[0].score.toFixed(3)} · margin ${result.margin.toFixed(3)}`
    : "No candidate satisfies the query.";
  for (const w of warnings) $("status").innerHTML += `<span class="warn">${w.replace("warning: ", "")}</span>`;
  $("query").textContent = JSON.stringify(result.query, null, 2);
  $("explanation").textContent = result.explanation;
  showTrace(result, result.candidates[0]?.id);
}

function run(text) {
  $("input").value = text;
  try {
    showResult(JSON.parse(state.resolver.resolve(text)));
  } catch (e) {
    state.result = null;
    $("status").innerHTML = `<span class="err">${String(e.message ?? e)}</span>`;
    $("candidates").querySelector("tbody").innerHTML = "";
    $("trace").innerHTML = "";
    $("query").textContent = "";
    $("explanation").textContent = "";
    paint(null, null);
  }
}

async function loadScene(name) {
  const cfg = SCENES[name];
  const json = await (await fetch(cfg.url)).text();
  state.resolver?.free();
  state.resolver = new WasmResolver(json);
  state.scene = JSON.parse(json);
  state.name = name;
  $("scene").value = name;
  $("examples").innerHTML = "";
  for (const ex of cfg.examples) {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = ex;
    b.addEventListener("click", () => run(ex));
    $("examples").appendChild(b);
  }
  buildWorld(state.scene);
  run(cfg.examples[0]);
}

function orbit(deg) {
  const off = camera.position.clone().sub(controls.target);
  off.applyAxisAngle(new THREE.Vector3(0, 0, 1), THREE.MathUtils.degToRad(deg));
  camera.position.copy(controls.target).add(off);
  controls.update();
  render();
}

async function main() {
  await init();
  $("version").textContent = `v${version()}`;
  for (const name of Object.keys(SCENES)) {
    const o = document.createElement("option");
    o.value = o.textContent = name;
    $("scene").appendChild(o);
  }
  $("scene").addEventListener("change", (e) => loadScene(e.target.value));
  $("form").addEventListener("submit", (e) => {
    e.preventDefault();
    run($("input").value);
  });
  resize();
  const params = new URLSearchParams(location.search);
  await loadScene(params.get("scene") && SCENES[params.get("scene")] ? params.get("scene") : "Living room");
  if (params.get("q")) run(params.get("q"));
}

// Hooks for headless screenshots and debugging.
window.__demo = { ready: main(), run, loadScene, orbit, render, state };
