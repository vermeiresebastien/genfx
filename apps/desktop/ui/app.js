// Flux node-graph editor frontend. Talks to the Rust command surface via Tauri
// IPC. Rust owns all state/validation/rendering; this UI draws the node canvas,
// submits graph edits, and shows preview frames. It never runs its own engine.

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
  graph: null,          // active graph object { id, nodes, edges, outputNode }
  activeGraphId: null,
  selectedNode: null,
  time: 0,
  playing: false,
  previewTimer: null,
};

const NODE_W = 170;
const PORT_H = 20;
const HDR_H = 30;

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
    setStatus(`${res.error.code}: ${res.error.message}`);
    return res;
  }
  applySnapshot(res);
  return res;
}

function applySnapshot(res) {
  state.revision = res.revision;
  document.getElementById("revBadge").textContent = `rev ${res.revision}`;
  const r = res.result || {};
  const p = r.project ?? r;
  if (p && p.projectId) {
    state.project = p;
    document.getElementById("projName").textContent = p.name;
  }
  if (r.activeGraphId) state.activeGraphId = r.activeGraphId;
  if (r.activeGraph) state.graph = r.activeGraph;
  renderCanvas();
  renderInspector();
}

async function loadCatalog() {
  const res = await invoke("dispatch", {
    request: {
      protocolVersion: 1, requestId: rid(), projectId: null,
      expectedRevision: 0, command: { type: "catalog" },
    },
  });
  if (res.result?.entries) {
    state.catalog = res.result.entries;
    renderLibrary("");
  }
}

function manifestOf(typeId) {
  return state.catalog.find((e) => e.id === typeId);
}

function renderLibrary(filter) {
  const f = (filter || "").toLowerCase();
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
      d.onclick = () => addNode(e.id);
      host.appendChild(d);
    }
  }
}

// ---- Node canvas ---------------------------------------------------------

let _nodeSeq = 0;

async function addNode(typeId) {
  // Place new nodes in a loose grid near the top-left of the canvas.
  const x = 40 + (_nodeSeq % 3) * 220;
  const y = 40 + Math.floor(_nodeSeq / 3) * 140;
  _nodeSeq++;
  const res = await call({ type: "addGraphNode", typeId, x, y });
  const id = res.result?.nodeId;
  if (id) {
    state.selectedNode = id;
    // Auto-wire the FIRST generator to the output node for convenience.
    maybeAutoWire(id, typeId);
    requestPreview();
  }
}

async function maybeAutoWire(newId, typeId) {
  const g = state.graph;
  if (!g) return;
  const m = manifestOf(typeId);
  if (!m) return;
  const out = g.nodes.find((n) => n.typeId === "flux.output");
  if (!out) return;
  const outEdge = (g.edges || []).find((e) => e.toNode === out.id && e.toPort === "source");
  const provider = outEdge ? outEdge.fromNode : null;

  if (m.kind === "generator") {
    // A generator becomes the new source feeding the output (so you immediately
    // see what you clicked). Rewire output.source -> new generator.
    if (outEdge) await call({ type: "disconnectPin", toNode: out.id, toPort: "source" });
    await call({ type: "connectPins", fromNode: newId, fromPort: "color", toNode: out.id, toPort: "source" });
    return;
  }

  // An effect: insert it into the chain (provider -> effect -> output) when it has
  // a single "source" input and there is an existing provider. Two-input nodes
  // (composite) are left unconnected for manual wiring.
  const hasSource = (m.inputs || []).some((p) => p.id === "source");
  if (hasSource && provider && provider !== newId) {
    await call({ type: "disconnectPin", toNode: out.id, toPort: "source" });
    await call({ type: "connectPins", fromNode: provider, fromPort: "color", toNode: newId, toPort: "source" });
    await call({ type: "connectPins", fromNode: newId, fromPort: "color", toNode: out.id, toPort: "source" });
  }
}

function nodeBox(node) {
  const m = manifestOf(node.typeId);
  const inputs = m?.inputs || [];
  const outputs = m?.outputs || [];
  return {
    x: node.position?.[0] ?? 40,
    y: node.position?.[1] ?? 40,
    inputs,
    outputs,
    w: NODE_W,
    h: HDR_H + Math.max(inputs.length, outputs.length) * PORT_H + 14,
  };
}

function portPos(node, kind, portId) {
  const b = nodeBox(node);
  const m = manifestOf(node.typeId);
  const ports = kind === "in" ? (m?.inputs || []) : (m?.outputs || []);
  const idx = Math.max(0, ports.findIndex((p) => p.id === portId));
  const y = b.y + HDR_H + idx * PORT_H + PORT_H / 2;
  const x = kind === "in" ? b.x : b.x + b.w;
  return { x, y };
}

function renderCanvas() {
  const canvas = document.getElementById("canvas");
  const g = state.graph;
  // Remove existing node elements (keep the <svg>).
  for (const el of [...canvas.querySelectorAll(".node")]) el.remove();
  const svg = document.getElementById("edges");
  while (svg.firstChild) svg.removeChild(svg.firstChild);
  if (!g) return;
  if (!g.nodes.some((n) => n.id === state.selectedNode)) state.selectedNode = state.selectedNode;

  // Draw edges first (behind nodes).
  for (const e of g.edges || []) {
    const from = g.nodes.find((n) => n.id === e.fromNode);
    const to = g.nodes.find((n) => n.id === e.toNode);
    if (!from || !to) continue;
    const a = portPos(from, "out", e.fromPort);
    const b = portPos(to, "in", e.toPort);
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("class", "edge");
    path.setAttribute("d", bezier(a, b));
    path.onclick = (ev) => {
      ev.stopPropagation();
      disconnect(e.toNode, e.toPort);
    };
    svg.appendChild(path);
  }

  // Draw nodes.
  for (const node of g.nodes) {
    const m = manifestOf(node.typeId);
    const b = nodeBox(node);
    const el = document.createElement("div");
    el.className = "node" + (node.id === state.selectedNode ? " sel" : "");
    el.style.left = b.x + "px";
    el.style.top = b.y + "px";
    el.dataset.nodeId = node.id;

    const hdr = document.createElement("div");
    hdr.className = "hdr";
    const isOut = node.typeId === "flux.output";
    hdr.innerHTML = `${m?.name ?? node.typeId}${isOut ? " ★" : ""} <small>${m?.kind ?? ""}</small>`;
    hdr.onmousedown = (ev) => startNodeDrag(ev, node);
    el.appendChild(hdr);

    const ports = document.createElement("div");
    ports.className = "ports";
    const ins = m?.inputs || [];
    const outs = m?.outputs || [];
    const rows = Math.max(ins.length, outs.length, 1);
    for (let i = 0; i < rows; i++) {
      const row = document.createElement("div");
      row.style.position = "relative";
      row.style.height = PORT_H + "px";
      if (ins[i]) {
        const dot = document.createElement("div");
        dot.className = "dot";
        dot.style.left = "-6px"; dot.style.top = (PORT_H / 2 - 5) + "px"; dot.style.position = "absolute";
        dot.dataset.port = ins[i].id; dot.dataset.kind = "in"; dot.dataset.node = node.id;
        dot.onmouseup = (ev) => endLink(ev, node.id, ins[i].id);
        row.appendChild(dot);
        const lbl = document.createElement("span");
        lbl.className = "lbl"; lbl.textContent = ins[i].id;
        lbl.style.position = "absolute"; lbl.style.left = "8px";
        row.appendChild(lbl);
      }
      if (outs[i]) {
        const dot = document.createElement("div");
        dot.className = "dot";
        dot.style.right = "-6px"; dot.style.top = (PORT_H / 2 - 5) + "px"; dot.style.position = "absolute";
        dot.dataset.port = outs[i].id; dot.dataset.kind = "out"; dot.dataset.node = node.id;
        dot.onmousedown = (ev) => startLink(ev, node.id, outs[i].id);
        row.appendChild(dot);
        const lbl = document.createElement("span");
        lbl.className = "lbl"; lbl.textContent = outs[i].id;
        lbl.style.position = "absolute"; lbl.style.right = "8px";
        row.appendChild(lbl);
      }
      ports.appendChild(row);
    }
    el.appendChild(ports);
    el.onmousedown = (ev) => {
      if (ev.target.classList.contains("dot")) return;
      selectNode(node.id);
    };
    canvas.appendChild(el);
  }
}

function bezier(a, b) {
  const dx = Math.max(40, Math.abs(b.x - a.x) * 0.5);
  return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
}

function selectNode(id) {
  state.selectedNode = id;
  renderCanvas();
  renderInspector();
}

async function disconnect(toNode, toPort) {
  await call({ type: "disconnectPin", toNode, toPort });
  requestPreview();
}

// Node drag.
let _drag = null;
function startNodeDrag(ev, node) {
  ev.stopPropagation();
  selectNode(node.id);
  const b = nodeBox(node);
  _drag = { id: node.id, ox: ev.clientX, oy: ev.clientY, nx: b.x, ny: b.y, moved: false };
  window.addEventListener("mousemove", onNodeDrag);
  window.addEventListener("mouseup", endNodeDrag);
}
function onNodeDrag(ev) {
  if (!_drag) return;
  const dx = ev.clientX - _drag.ox;
  const dy = ev.clientY - _drag.oy;
  if (Math.abs(dx) + Math.abs(dy) > 3) _drag.moved = true;
  const node = state.graph?.nodes.find((n) => n.id === _drag.id);
  if (node) {
    node.position = [ _drag.nx + dx, _drag.ny + dy ];
    renderCanvas();
  }
}
async function endNodeDrag() {
  window.removeEventListener("mousemove", onNodeDrag);
  window.removeEventListener("mouseup", endNodeDrag);
  if (_drag && _drag.moved) {
    const node = state.graph?.nodes.find((n) => n.id === _drag.id);
    if (node) {
      await call({ type: "moveNode", nodeId: node.id, x: node.position[0], y: node.position[1] });
    }
  }
  _drag = null;
}

// Port link drag.
let _link = null;
function startLink(ev, nodeId, port) {
  ev.stopPropagation();
  ev.preventDefault();
  _link = { node: nodeId, port };
  const svg = document.getElementById("edges");
  const tmp = document.createElementNS("http://www.w3.org/2000/svg", "path");
  tmp.setAttribute("class", "temp");
  tmp.id = "tempLink";
  svg.appendChild(tmp);
  window.addEventListener("mousemove", onLinkDrag);
  window.addEventListener("mouseup", cancelLink);
}
function onLinkDrag(ev) {
  if (!_link) return;
  const node = state.graph?.nodes.find((n) => n.id === _link.node);
  if (!node) return;
  const a = portPos(node, "out", _link.port);
  const rect = document.getElementById("canvas").getBoundingClientRect();
  const b = { x: ev.clientX - rect.left, y: ev.clientY - rect.top };
  const tmp = document.getElementById("tempLink");
  if (tmp) tmp.setAttribute("d", bezier(a, b));
}
function cancelLink() {
  window.removeEventListener("mousemove", onLinkDrag);
  window.removeEventListener("mouseup", cancelLink);
  const tmp = document.getElementById("tempLink");
  if (tmp) tmp.remove();
  _link = null;
}
async function endLink(ev, toNode, toPort) {
  ev.stopPropagation();
  if (!_link) return;
  const from = _link;
  cancelLink();
  await call({
    type: "connectPins",
    fromNode: from.node, fromPort: from.port,
    toNode, toPort,
  });
  requestPreview();
}

// ---- Inspector -----------------------------------------------------------

function renderInspector() {
  const host = document.getElementById("inspector");
  const g = state.graph;
  if (!g || !state.selectedNode) {
    host.innerHTML = `<div class="muted">Select a node.</div>`;
    return;
  }
  const node = g.nodes.find((n) => n.id === state.selectedNode);
  if (!node) { host.innerHTML = `<div class="muted">Node not found.</div>`; return; }
  const m = manifestOf(node.typeId);
  host.innerHTML = `<div><strong>${m?.name ?? node.typeId}</strong></div>
    <div class="muted">${node.typeId}${node.typeId === "flux.output" ? " (output)" : ""}</div><hr/>`;
  const decls = (m?.parameters || []).filter((d) => d.id !== "wet");
  const overrides = node.params || {};
  if (decls.length === 0) {
    host.innerHTML += `<div class="muted">No editable parameters.</div>`;
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
    sel.value = typeof current === "number" ? current : 0;
    sel.onchange = () => setParam(node, paramId, Number(sel.value), "int");
    row.appendChild(sel);
  } else if (kind === "color") {
    const arr = Array.isArray(current) ? current : [1, 1, 1, 1];
    const wrap = document.createElement("div");
    wrap.style.display = "flex";
    wrap.style.gap = "3px";
    for (let i = 0; i < 4; i++) {
      const c = document.createElement("input");
      c.type = "number"; c.step = "0.01"; c.min = "0"; c.max = "1";
      c.value = arr[i] ?? (i === 3 ? 1 : 0);
      c.style.width = "100%";
      c.onchange = () => {
        const v = arr.slice();
        v[i] = Number(c.value);
        setColorParam(node, paramId, v);
      };
      wrap.appendChild(c);
    }
    row.appendChild(wrap);
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
  await call({
    type: "apply",
    command: { type: "setParameter", graphId: state.activeGraphId, nodeId: node.id, paramId, value: pv },
  });
  requestPreview();
}

async function setColorParam(node, paramId, rgba) {
  await call({
    type: "apply",
    command: {
      type: "setParameter",
      graphId: state.activeGraphId, nodeId: node.id, paramId,
      value: { kind: "color", value: rgba },
    },
  });
  requestPreview();
}

// ---- Preview -------------------------------------------------------------

let _previewBusy = false;
let _previewPending = false;

async function requestPreview() {
  if (_previewBusy) { _previewPending = true; return; }
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
    if (_previewPending) { _previewPending = false; requestPreview(); }
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

// ---- Playback + misc -----------------------------------------------------

function timelineSeconds() {
  const d = state.graph?.id ? (state.project?.timeline?.range?.duration) : null;
  const s = d ? d.numerator / d.denominator : 8;
  return s > 0 ? s : 8;
}
function clampTime() {
  const dur = timelineSeconds();
  if (state.time < 0) state.time = 0;
  if (state.time >= dur) state.time = state.time % dur;
}

function setupPlayback() {
  const btn = document.getElementById("playBtn");
  btn.onclick = () => {
    state.playing = !state.playing;
    btn.textContent = state.playing ? "Pause" : "Play";
    if (state.playing) {
      state.previewTimer = setInterval(() => {
        state.time += 1 / 30;
        clampTime();
        requestPreview();
      }, 100);
    } else {
      clearInterval(state.previewTimer);
    }
  };
}

function setupMisc() {
  document.getElementById("search").oninput = (e) => renderLibrary(e.target.value);
  document.getElementById("exportBtn").onclick = exportProject;
  document.getElementById("undoBtn").onclick = async () => { await call({ type: "undo" }); requestPreview(); };
  document.getElementById("redoBtn").onclick = async () => { await call({ type: "redo" }); requestPreview(); };
  document.addEventListener("keydown", async (e) => {
    if ((e.key === "Delete" || e.key === "Backspace") && state.selectedNode &&
        document.activeElement.tagName !== "INPUT" && document.activeElement.tagName !== "SELECT") {
      const id = state.selectedNode;
      state.selectedNode = null;
      await call({ type: "removeGraphNode", nodeId: id });
      requestPreview();
    }
  });
}

async function boot() {
  await call(
    { type: "createProject", name: "Untitled", seed: 1, width: 1280, height: 720, fpsNum: 60, fpsDen: 1 },
    0
  );
  await loadCatalog();
  setupPlayback();
  setupMisc();
  // Select the generator node (not the output) so the inspector is populated.
  const g = state.graph;
  if (g) {
    const n = g.nodes.find((x) => x.typeId !== "flux.output");
    if (n) state.selectedNode = n.id;
  }
  renderCanvas();
  renderInspector();
  requestPreview();
}

window.addEventListener("DOMContentLoaded", boot);
