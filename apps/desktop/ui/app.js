// Flux editor frontend. Talks to the Rust command surface via Tauri IPC.
// All state/validation/rendering is owned by Rust; this UI submits requests and
// renders snapshots + preview frames. It never runs its own engine.

const invoke = window.__TAURI__.core.invoke;

function rid() {
  try {
    if (crypto && typeof crypto.randomUUID === "function") return crypto.randomUUID();
  } catch (e) {}
  return "r-" + Math.random().toString(36).slice(2) + Date.now().toString(36);
}

window.addEventListener("error", (e) => {
  const s = document.getElementById("status");
  if (s) s.textContent = "UI error: " + (e.message || e.error);
});
window.addEventListener("unhandledrejection", (e) => {
  const s = document.getElementById("status");
  if (s) s.textContent = "UI error: " + (e.reason && e.reason.message ? e.reason.message : e.reason);
});

const state = {
  revision: 0,
  project: null,
  catalog: [],
  selectedNode: null,
  selectedClip: null,
  time: 0,
  playing: false,
  previewTimer: null,
};

function setStatus(msg) {
  document.getElementById("status").textContent = msg;
}

async function call(command, expectedRevision = state.revision) {
  const req = {
    protocolVersion: 1,
    requestId: rid(),
    projectId: state.project?.projectId ?? null,
    expectedRevision,
    command,
  };
  const res = await invoke("dispatch", { request: req });
  if (res.error) {
    setStatus(`${res.error.code}: ${res.error.message}${res.error.recovery ? " — " + res.error.recovery : ""}`);
    return res;
  }
  applySnapshot(res);
  return res;
}

function applySnapshot(res) {
  state.revision = res.revision;
  document.getElementById("revBadge").textContent = `rev ${res.revision}`;
  const p = res.result?.project ?? res.result;
  if (p && p.projectId) {
    state.project = p;
    document.getElementById("projName").textContent = p.name;
    renderTimeline();
    document.getElementById("history").textContent =
      `${res.result?.canUndo ? "undo" : ""} ${res.result?.canRedo ? "redo" : ""}`.trim();
  }
}

async function loadCatalog() {
  const res = await invoke("dispatch", {
    request: {
      protocolVersion: 1, requestId: crypto.randomUUID(), projectId: null,
      expectedRevision: 0, command: { type: "catalog" },
    },
  });
  if (res.result?.entries) {
    state.catalog = res.result.entries;
    renderLibrary("");
  }
}

function renderLibrary(filter) {
  const f = filter.toLowerCase();
  const byCat = {};
  for (const e of state.catalog) {
    if (f && !e.name.toLowerCase().includes(f) && !e.id.includes(f)) continue;
    (byCat[e.category] ||= []).push(e);
  }
  const host = document.getElementById("library");
  host.innerHTML = "";
  for (const [cat, items] of Object.entries(byCat).sort()) {
    const h = document.createElement("div");
    h.className = "lib-cat";
    h.textContent = cat;
    host.appendChild(h);
    for (const e of items) {
      const d = document.createElement("div");
      d.className = "lib-item";
      d.innerHTML = `<span>${e.name}</span><span class="muted">${e.kind}</span>`;
      d.onclick = () => addScene(e);
      host.appendChild(d);
    }
  }
}

async function addScene(entry) {
  // Add a generator clip with an effect chain when the entry is a generator.
  const effects = entry.kind === "generator" ? [] : [entry.id];
  const gen = entry.kind === "generator" ? entry.id : "gen.plasma";
  await call({
    type: "addGeneratorClip",
    generatorId: gen,
    effects,
    seconds: 8,
  });
  state.selectedNode = null;
  renderInspector();
  requestPreview();
}

function renderTimeline() {
  const host = document.getElementById("timeline");
  host.innerHTML = "";
  const p = state.project;
  if (!p) return;
  document.getElementById("rangeInfo").textContent =
    `${p.timeline.range.duration.numerator}/${p.timeline.range.duration.denominator}s`;
  for (const tid of p.timeline.trackOrder) {
    const track = p.timeline.tracks[tid];
    const row = document.createElement("div");
    row.className = "row";
    row.innerHTML = `<span class="muted" style="width:120px">${track.name}</span>`;
    for (const cid of track.clipOrder) {
      const clip = p.timeline.clips[cid];
      const el = document.createElement("div");
      el.className = "clip";
      el.innerHTML = `<div>${clip.name}</div><div class="muted">${clip.range.duration.numerator}/${clip.range.duration.denominator}s</div>`;
      el.onclick = () => selectClip(tid, cid);
      row.appendChild(el);
    }
    host.appendChild(row);
  }
}

function selectClip(tid, cid) {
  state.selectedClip = cid;
  const clip = state.project.timeline.clips[cid];
  if (clip.graphId && state.project.graphs[clip.graphId]) {
    const g = state.project.graphs[clip.graphId];
    state.selectedNode = g.nodes.find((n) => n.typeId !== "flux.output")?.id ?? null;
  }
  renderInspector();
}

function renderInspector() {
  const host = document.getElementById("inspector");
  const p = state.project;
  if (!p || !state.selectedNode) {
    host.innerHTML = `<div class="muted">Select a node.</div>`;
    return;
  }
  let node = null;
  for (const g of Object.values(p.graphs)) {
    const n = g.nodes.find((x) => x.id === state.selectedNode);
    if (n) { node = n; break; }
  }
  if (!node) { host.innerHTML = `<div class="muted">Node not found.</div>`; return; }
  const manifest = state.catalog.find((e) => e.id === node.typeId);
  const overrides = node.params || {};
  host.innerHTML = `<div><strong>${manifest?.name ?? node.typeId}</strong></div>
    <div class="muted">${node.typeId} v${node.typeVersion}</div><hr/>`;
  // Render controls from the catalog manifest's parameter declarations,
  // overlaid with this node's stored overrides. Nodes without a catalog entry
  // fall back to showing their raw params.
  const decls = manifest?.parameters || [];
  if (decls.length === 0) {
    const params = overrides;
    if (Object.keys(params).length === 0) {
      host.innerHTML += `<div class="muted">No editable parameters.</div>`;
    }
    for (const [k, v] of Object.entries(params)) {
      host.appendChild(makeParamRow(node, k, (v && v.kind) || "float", v && v.value, {}));
    }
    return;
  }
  for (const d of decls) {
    const ov = overrides[d.id];
    const current = ov !== undefined ? ov.value : d.default;
    host.appendChild(makeParamRow(node, d.id, d.kind, current, d));
  }
}

function makeParamRow(node, paramId, kind, current, decl) {
  const row = document.createElement("div");
  row.className = "inspector-param";
  const label = document.createElement("label");
  label.textContent = decl.label || paramId;
  row.appendChild(label);
  if (kind === "float" || kind === "int") {
    const input = document.createElement("input");
    input.type = "number";
    input.value = current;
    input.step = kind === "int" ? "1" : "0.01";
    if (decl.min != null) input.min = decl.min;
    if (decl.max != null) input.max = decl.max;
    input.onchange = () => setParam(node, paramId, Number(input.value), kind);
    row.appendChild(input);
  } else if (kind === "bool") {
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = !!current;
    input.onchange = () => setParam(node, paramId, input.checked, "bool");
    row.appendChild(input);
  } else if (kind === "enum") {
    const sel = document.createElement("select");
    (decl.options || []).forEach((opt, i) => {
      const o = document.createElement("option");
      o.value = i; o.textContent = opt;
      sel.appendChild(o);
    });
    sel.value = current;
    sel.onchange = () => setParam(node, paramId, Number(sel.value), "int");
    row.appendChild(sel);
  } else {
    const span = document.createElement("span");
    span.className = "muted";
    span.textContent = JSON.stringify(current);
    row.appendChild(span);
  }
  return row;
}

async function setParam(node, paramId, value, kind) {
  const pv = kind === "bool"
    ? { kind: "bool", value }
    : kind === "int"
    ? { kind: "int", value: Math.round(value) }
    : { kind: "float", value };
  const graphId = Object.entries(state.project.graphs).find(([, g]) =>
    g.nodes.some((n) => n.id === node.id))?.[0];
  await call({
    type: "apply",
    command: { type: "setParameter", graphId, nodeId: node.id, paramId, value: pv },
  });
  requestPreview();
}

let _previewBusy = false;
let _previewPending = false;

async function requestPreview() {
  // Single-flight + coalesce: never more than one render in flight. If a render
  // is already running, mark pending and let it re-run once when done. This
  // prevents the render queue from backing up (each render is heavy: it reuses
  // the GPU path and can take 100ms+).
  if (_previewBusy) {
    _previewPending = true;
    return;
  }
  _previewBusy = true;
  try {
    const url = await invoke("render_preview", {
      timeSeconds: state.time,
      width: 640,
      height: 360,
    });
    document.getElementById("preview").src = url;
  } catch (e) {
    setStatus(`preview: ${e}`);
  } finally {
    _previewBusy = false;
    if (_previewPending) {
      _previewPending = false;
      requestPreview();
    }
  }
}

async function exportProject() {
  const outDir = document.getElementById("outDir").value || "out";
  setStatus("exporting…");
  try {
    const msg = await invoke("export_project_cmd", { outDir, width: 1280, height: 720 });
    setStatus(msg);
  } catch (e) {
    setStatus(`export failed: ${e}`);
  }
}

function setupPlayback() {
  const btn = document.getElementById("playBtn");
  btn.onclick = () => {
    state.playing = !state.playing;
    btn.textContent = state.playing ? "Pause" : "Play";
    if (state.playing) {
      // Playback preview at ~10fps (not 30): each frame is a real GPU render,
      // and 30fps of full renders saturates the CPU. Single-flight prevents
      // queue buildup if a render runs long.
      state.previewTimer = setInterval(() => {
        state.time += 1 / 30;
        requestPreview();
      }, 100);
    } else {
      clearInterval(state.previewTimer);
    }
  };
  document.getElementById("prevBtn").onclick = () => {
    state.time = Math.max(0, state.time - 1 / 30);
    requestPreview();
  };
  document.getElementById("nextBtn").onclick = () => {
    state.time += 1 / 30;
    requestPreview();
  };
}

function setupMisc() {
  document.getElementById("search").oninput = (e) => renderLibrary(e.target.value);
  document.getElementById("exportBtn").onclick = exportProject;
  document.getElementById("undoBtn").onclick = async () => {
    await call({ type: "undo" }, 0);
    requestPreview();
  };
  document.getElementById("redoBtn").onclick = async () => {
    await call({ type: "redo" }, 0);
    requestPreview();
  };
}

async function boot() {
  // Create the initial project through the engine.
  await call(
    { type: "createProject", name: "Untitled", seed: 1, width: 1280, height: 720, fpsNum: 60, fpsDen: 1 },
    0
  );
  await loadCatalog();
  setupPlayback();
  setupMisc();
  // Add one starter scene so the preview shows something.
  const plasma = state.catalog.find((e) => e.id === "gen.plasma");
  if (plasma) await addScene({ ...plasma, kind: "generator" });
  // Auto-select the first editable node so the inspector is populated.
  selectFirstNode();
  renderInspector();
  requestPreview();
}

function selectFirstNode() {
  const p = state.project;
  if (!p) return;
  for (const g of Object.values(p.graphs)) {
    const n = g.nodes.find((x) => x.typeId !== "flux.output");
    if (n) { state.selectedNode = n.id; return; }
  }
}

window.addEventListener("DOMContentLoaded", boot);
