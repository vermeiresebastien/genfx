// Flux node-graph editor entry. Boots the engine, wires the view modules, and
// owns the keyboard/transport/preview loops. No engine logic lives here — every
// edit is a command and every frame is rendered by Rust.

import {
  state, subscribe, setCatalog, syncCanvasMeta, manifestOf, selectedNodes,
} from "./store.js";
import * as commands from "./commands.js";
import { renderPreview, renderGraphPreview, exportProject, listen } from "./ipc.js";
import { initCanvas } from "./view/canvas.js";
import { initLibrary, render as renderLibrary } from "./view/library.js";
import { initInspector, render as renderInspector } from "./view/inspector.js";
import { initTimeline, render as renderTimeline } from "./view/timeline.js";
import { initOverlay } from "./view/overlay.js";
import { nodeRect } from "./view/node.js";

let canvas;
let library;
let inspector;
let timeline;
let overlay;

// Surface any runtime error in the status bar instead of failing silently.
window.addEventListener("error", (e) => {
  const el = document.querySelector("#status .msg");
  if (el) { el.textContent = "UI error: " + (e.message || e.error); el.classList.add("err"); }
});
window.addEventListener("unhandledrejection", (e) => {
  const el = document.querySelector("#status .msg");
  if (el) { el.textContent = "UI error: " + (e.reason?.message || e.reason); el.classList.add("err"); }
});

// ---- status ----------------------------------------------------------------

const statusMsgEl = () => document.querySelector("#status .msg");
const statusStatEl = () => document.querySelector("#status .stat");

function setStatus(msg, isErr = false) {
  const el = statusMsgEl();
  if (!el) return;
  el.textContent = msg;
  el.classList.toggle("err", !!isErr);
}

function updateStats() {
  const el = statusStatEl();
  if (!el) return;
  const g = state.graph;
  const z = Math.round((state.view?.zoom || 1) * 100);
  el.textContent = `rev ${state.revision} · ${g ? g.nodes.length + "n " + g.edges.length + "e" : "no graph"} · ${z}%`;
  const badge = document.getElementById("revBadge");
  if (badge) badge.textContent = `rev ${state.revision}`;
  const name = document.getElementById("projName");
  if (name && state.project?.name) name.textContent = state.project.name;
}

// ---- connect with validation ---------------------------------------------

/** Would connecting `from` -> `to` close a cycle? */
function reaches(from, target) {
  const g = state.graph;
  if (!g) return false;
  const seen = new Set();
  const stack = [from];
  while (stack.length) {
    const id = stack.pop();
    if (id === target) return true;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const e of g.edges) if (e.fromNode === id) stack.push(e.toNode);
  }
  return false;
}

function isTemporalNode(id) {
  const n = state.graph?.nodes.find((x) => x.id === id);
  return !!n && (n.typeId === "flux.feedback_read" || n.typeId === "flux.feedback_write");
}

/** Connect, replacing any existing edge on the destination input and refusing
 *  cycles (except legal feedback read/write cycles). */
async function replaceConnect(fromNode, fromPort, toNode, toPort) {
  if (fromNode !== toNode && reaches(toNode, fromNode) && !(isTemporalNode(fromNode) || isTemporalNode(toNode))) {
    setStatus("Connection refused: would create a cycle", true);
    return;
  }
  const existing = (state.graph?.edges || []).find((e) => e.toNode === toNode && e.toPort === toPort);
  if (existing && !(existing.fromNode === fromNode && existing.fromPort === fromPort)) {
    await commands.disconnect(toNode, toPort);
  }
  await commands.connect(fromNode, fromPort, toNode, toPort);
}

// ---- render orchestration -------------------------------------------------

let renderScheduled = false;
function scheduleRender() {
  if (renderScheduled) return;
  renderScheduled = true;
  requestAnimationFrame(() => {
    renderScheduled = false;
    canvas?.render();
    renderInspector();
    renderTimeline();
    updateStats();
  });
}

function renderAll() {
  canvas?.render();
  renderInspector();
  renderTimeline();
  updateStats();
}

// ---- previews (Output nodes + detached monitor) ---------------------------

const thumbCache = new Map();
let previewBusy = false;
let previewPending = false;

function setThumb(id, url) {
  thumbCache.set(id, url);
  const img = document.querySelector(`img[data-thumb-for="${id}"]`);
  if (img) img.src = url;
}

function fillThumb(img, nodeId) {
  const url = thumbCache.get(nodeId);
  if (url) img.src = url;
}

function requestPreviews() {
  if (previewBusy) { previewPending = true; return; }
  previewBusy = true;
  (async () => {
    try {
      const g = state.graph;
      const outputs = (g?.nodes || []).filter((n) => n.typeId === "flux.output");
      for (const n of outputs) {
        const url = await renderGraphPreview(state.activeGraphId, state.time, 320, 180, n.id);
        setThumb(n.id, url);
      }
      if (overlay?.isMonitorOpen()) {
        const url = await renderPreview(state.time, 640, 360);
        overlay.setMonitorImage(url);
      }
    } catch (e) {
      setStatus("preview: " + (e?.message || e), true);
    } finally {
      previewBusy = false;
      if (previewPending) { previewPending = false; requestPreviews(); }
    }
  })();
}

// ---- adding nodes ---------------------------------------------------------

function centerWorld() {
  const vp = document.getElementById("viewport");
  const r = vp.getBoundingClientRect();
  return canvas.toWorld(r.left + r.width / 2, r.top + r.height / 2);
}

function newId() {
  try { return crypto.randomUUID(); } catch (e) { return "n-" + Math.random().toString(36).slice(2) + Date.now().toString(36); }
}

async function addAt(typeId, pt, skipAutoWire = false) {
  const res = await commands.addNode(typeId, pt.x, pt.y);
  const id = res?.result?.nodeId;
  if (!id) return null;
  state.selection = new Set([id]);
  if (!skipAutoWire) await autoWire(id, typeId);
  renderAll();
  requestPreviews();
  return id;
}

/** Wire a newly-created node into the composition sensibly:
 *  - a generator with no source feeding Output becomes the output's source;
 *  - a single-input effect is inserted between the current source and Output. */
async function autoWire(newId, typeId) {
  const g = state.graph;
  const m = manifestOf(typeId);
  if (!g || !m) return;
  const out = g.nodes.find((n) => n.typeId === "flux.output");
  if (!out) return;
  const outEdge = g.edges.find((e) => e.toNode === out.id && e.toPort === "source");
  const provider = outEdge ? outEdge.fromNode : null;

  if (m.kind === "generator") {
    if (outEdge) await commands.disconnect(out.id, "source");
    await commands.connect(newId, "color", out.id, "source");
    return;
  }

  const hasSource = (m.inputs || []).some((p) => p.id === "source");
  if (hasSource && provider && provider !== newId) {
    await commands.disconnect(out.id, "source");
    await commands.connect(provider, "color", newId, "source");
    await commands.connect(newId, "color", out.id, "source");
  }
}

// ---- duplicate ------------------------------------------------------------

async function duplicateSelection(ids) {
  const g = state.graph;
  if (!g || !ids.length) return;
  const map = new Map();
  const created = [];
  for (const id of ids) {
    const n = g.nodes.find((x) => x.id === id);
    if (!n) continue;
    const nid = newId();
    map.set(id, nid);
    const copy = {
      ...JSON.parse(JSON.stringify(n)),
      id: nid,
      position: [n.position[0] + 28, n.position[1] + 28],
    };
    created.push(copy);
    await commands.addNodeObj(copy);
  }
  for (const e of g.edges) {
    if (map.has(e.fromNode) && map.has(e.toNode)) {
      await commands.connect(map.get(e.fromNode), e.fromPort, map.get(e.toNode), e.toPort);
    }
  }
  state.selection = new Set(created.map((c) => c.id));
  renderAll();
  requestPreviews();
}

// ---- reroute on cable double-click ---------------------------------------

async function insertReroute(edge) {
  const g = state.graph;
  const from = g?.nodes.find((n) => n.id === edge.fromNode);
  const to = g?.nodes.find((n) => n.id === edge.toNode);
  if (!from || !to) return;
  const mx = (from.position[0] + to.position[0]) / 2 + 88;
  const my = (from.position[1] + to.position[1]) / 2;
  const id = await addAt("flux.reroute", { x: mx, y: my });
  if (!id) return;
  await commands.disconnect(edge.toNode, edge.toPort);
  await commands.connect(edge.fromNode, edge.fromPort, id, "source");
  await commands.connect(id, "color", edge.toNode, edge.toPort);
  state.selection = new Set([id]);
  renderAll();
}

// ---- frames ---------------------------------------------------------------

function frameSelection() {
  const g = state.graph;
  const nodes = selectedNodes();
  if (!g || !nodes.length) return;
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const n of nodes) {
    const r = nodeRect(n, manifestOf(n.typeId));
    minX = Math.min(minX, r.x); minY = Math.min(minY, r.y);
    maxX = Math.max(maxX, r.x + r.w); maxY = Math.max(maxY, r.y + r.h);
  }
  const pad = 22;
  state.frames.push({
    id: newId(),
    title: "Frame",
    rect: [minX - pad, minY - pad, maxX - minX + pad * 2, maxY - minY + pad * 2],
  });
  persistCanvasMeta();
  canvas.render();
}

let metaTimer = null;
function persistCanvasMeta() {
  clearTimeout(metaTimer);
  metaTimer = setTimeout(() => {
    commands.setCanvasMeta(state.frames, state.view);
  }, 400);
}

// ---- context menu ---------------------------------------------------------

function onContextMenu(ev) {
  ev.preventDefault();
  const nodeEl = ev.target.closest?.(".node");
  if (nodeEl) {
    const id = nodeEl.dataset.nodeId;
    if (!state.selection.has(id)) { state.selection = new Set([id]); renderAll(); }
    const node = state.graph?.nodes.find((n) => n.id === id);
    overlay.openMenu(ev.clientX, ev.clientY, [
      { label: "Duplicate", fn: () => duplicateSelection([...state.selection]) },
      { label: node?.bypass ? "Un-bypass" : "Bypass", fn: () => commands.setBypass(id, !node?.bypass) },
      { label: "Set as output", fn: () => commands.setGraphOutput(id) },
      { label: "Delete", danger: true, fn: () => { state.selection.delete(id); commands.removeNodes([id]); } },
    ]);
    return;
  }
  overlay.openMenu(ev.clientX, ev.clientY, [
    { label: "Add node…", fn: () => overlay.open({ world: canvas.toWorld(ev.clientX, ev.clientY) }) },
    { label: "Frame selection", fn: frameSelection },
    { label: "Fit graph", fn: () => canvas.fit() },
    { label: state.ui.minimap ? "Hide minimap" : "Show minimap", fn: toggleMinimap },
  ]);
}

function toggleMinimap() {
  state.ui.minimap = !state.ui.minimap;
  document.getElementById("minimap").classList.toggle("hidden", !state.ui.minimap);
  canvas.drawMinimap();
}

// ---- transport ------------------------------------------------------------

let playTimer = null;
state.loopMode = (() => {
  try { return localStorage.getItem("flux.loopMode") || "forward"; } catch (e) { return "forward"; }
})();
state.playDir = 1;

function applyLoopButton() {
  const btn = document.getElementById("loopBtn");
  if (!btn) return;
  btn.textContent = state.loopMode === "pingpong" ? "Loop: Ping" : "Loop: Fwd";
  btn.classList.toggle("on", state.loopMode === "pingpong");
}

function toggleLoopMode() {
  state.loopMode = state.loopMode === "pingpong" ? "forward" : "pingpong";
  state.playDir = 1;
  try { localStorage.setItem("flux.loopMode", state.loopMode); } catch (e) {}
  applyLoopButton();
}

function playbackTick() {
  const dur = durationSeconds();
  const dt = 1 / 30;
  if (state.loopMode === "pingpong") {
    state.time += dt * state.playDir;
    if (state.time >= dur) {
      state.time = Math.max(0, dur - 0.001);
      state.playDir = -1;
    } else if (state.time <= 0) {
      state.time = 0;
      state.playDir = 1;
    }
  } else {
    state.time += dt;
    if (state.time >= dur) state.time = state.time % Math.max(0.001, dur);
  }
  requestPreviews();
  renderTimeline();
}

function startPlayback() {
  if (state.playing) return;
  state.playing = true;
  const btn = document.getElementById("playBtn");
  btn.textContent = "Pause";
  btn.classList.add("on");
  playTimer = setInterval(playbackTick, 100);
}

function stopPlayback() {
  state.playing = false;
  clearInterval(playTimer);
  playTimer = null;
  const btn = document.getElementById("playBtn");
  btn.textContent = "Play";
  btn.classList.remove("on");
}

function togglePlay() {
  if (state.playing) stopPlayback();
  else startPlayback();
}

function durationSeconds() {
  const d = state.project?.timeline?.range?.duration;
  if (!d) return 8;
  const s = Number(d.numerator) / d.denominator;
  return s > 0 ? s : 8;
}

function seek(t) {
  state.time = Math.max(0, Math.min(durationSeconds(), t));
  requestPreviews();
  renderTimeline();
}

// ---- keyboard -------------------------------------------------------------

function isTyping() {
  const a = document.activeElement;
  return a && (a.tagName === "INPUT" || a.tagName === "SELECT" || a.tagName === "TEXTAREA");
}

function onKeyDown(ev) {
  const mod = ev.ctrlKey || ev.metaKey;
  if (isTyping()) {
    if (ev.key === "Escape") document.activeElement.blur();
    return;
  }
  if (mod && ev.key.toLowerCase() === "z") {
    ev.preventDefault();
    (ev.shiftKey ? commands.redo() : commands.undo()).then(() => { renderAll(); requestPreviews(); });
    return;
  }
  if (mod && ev.key.toLowerCase() === "y") { ev.preventDefault(); commands.redo().then(renderAll); return; }
  if (mod && ev.key.toLowerCase() === "d") { ev.preventDefault(); duplicateSelection([...state.selection]); return; }
  if (mod && ev.key.toLowerCase() === "s") { ev.preventDefault(); saveNow(); return; }

  switch (ev.key) {
    case " ":
      ev.preventDefault();
      overlay.open({ world: centerWorld() });
      break;
    case "Tab":
      ev.preventDefault();
      document.body.classList.toggle("insp-hidden");
      break;
    case "1":
      canvas.fit();
      break;
    case "2":
      if (state.selection.size === 1) canvas.focusNode([...state.selection][0]);
      else canvas.fit();
      break;
    case "b":
    case "B": {
      const sel = selectedNodes();
      for (const n of sel) commands.setBypass(n.id, !n.bypass);
      break;
    }
    case "f":
    case "F":
      frameSelection();
      break;
    case "m":
    case "M":
      toggleMinimap();
      break;
    case "Delete":
    case "Backspace":
      if (state.selection.size) {
        const ids = [...state.selection];
        state.selection.clear();
        commands.removeNodes(ids).then(() => { renderAll(); requestPreviews(); });
      }
      break;
    case "Escape":
      overlay.close();
      overlay.closeMenu();
      break;
    default:
      break;
  }
}

// ---- save / load / export -------------------------------------------------

function saveNow() {
  const path = document.getElementById("savePath").value || "project.flux.json";
  commands.saveProject(path).then((res) => {
    if (res && !res.error) setStatus(`saved → ${path}`);
  });
  persistCanvasMeta();
}

function openFile() {
  const input = document.getElementById("openFile");
  input.value = "";
  input.click();
}

async function onOpenFile(ev) {
  const file = ev.target.files?.[0];
  if (!file) return;
  const text = await file.text();
  const res = await commands.loadProject(text);
  if (res && !res.error) {
    state.selection.clear();
    syncCanvasMeta();
    renderAll();
    requestPreviews();
    setStatus(`loaded ${file.name}`);
  }
}

async function onExport() {
  const outDir = document.getElementById("outDir").value || "out";
  const w = Number(document.getElementById("expW").value) || 1280;
  const h = Number(document.getElementById("expH").value) || 720;
  const codec = document.getElementById("exportCodec").value || "h264";
  const keepFrames = document.getElementById("keepFrames").checked;

  // Show the composition while it renders: open the monitor and keep the loop
  // playing for the duration of the export.
  if (!overlay.isMonitorOpen()) {
    overlay.toggleMonitor(true);
    document.getElementById("monitorBtn").classList.add("on");
  }
  startPlayback();

  setStatus(`exporting ${codec.toUpperCase()}…`);
  try {
    const msg = await exportProject(outDir, w, h, codec, keepFrames);
    if (msg === "export started") setStatus("exporting… rendering frames");
    else setStatus(String(msg));
  } catch (e) {
    setStatus("export failed: " + (e?.message || e), true);
  }
}

// ---- boot -----------------------------------------------------------------

function makeTabs() {
  document.querySelectorAll("#lib .tabs button").forEach((btn) => {
    if (btn.dataset.tab === state.ui.libTab) {
      document.querySelectorAll("#lib .tabs button").forEach((b) => b.classList.toggle("active", b === btn));
    }
  });
}

async function boot() {
  commands.setStatusSink((m) => setStatus(m, true));

  // 1. project + catalog
  await commands.createProject("Untitled", 1, 1280, 720, 60, 1);
  const cat = await commands.catalog();
  if (cat?.result?.entries) setCatalog(cat.result.entries);

  const presets = await commands.listPresets();
  if (presets?.result?.presets) state.presets = presets.result.presets;

  syncCanvasMeta();

  // 2. views
  canvas = initCanvas({
    store: { state },
    commands: { ...commands, replaceConnect },
    manifestOf,
    fillThumb,
    onSelectionChanged: () => { renderInspector(); canvas.render(); updateStats(); },
    onDuplicateSelection: duplicateSelection,
    onOpenSearch: (ctx) => overlay.open(ctx),
    onEdgeClick: (edge) => commands.disconnect(edge.toNode, edge.toPort),
    onEdgeDoubleClick: (edge) => insertReroute(edge),
    onViewChanged: () => { persistCanvasMeta(); updateStats(); },
    onStatus: (m) => setStatus(m, true),
    takePendingLink: () => pendingLink,
  });

  library = initLibrary({
    onAdd: (typeId) => addAt(typeId, centerWorld()),
    onInsertPreset: async (presetId, pt) => {
      await commands.insertPreset(presetId, pt || centerWorld());
      state.selection.clear();
      renderAll();
      requestPreviews();
    },
  });

  inspector = initInspector({
    commands,
    onFocusNode: (id) => canvas.focusNode(id),
  });

  timeline = initTimeline({
    onSeek: seek,
    onRemoveKeyframe: (lane, k) => {
      const keys = (state.graph?.keyframes?.[lane] || []).filter(
        (x) => !(x.time.numerator === k.time.numerator && x.time.denominator === k.time.denominator)
      );
      commands.setKeyframes(lane, keys);
    },
  });

  overlay = initOverlay({
    onPick: async (typeId, ctx) => {
      const pt = ctx?.world || centerWorld();
      if (ctx?.link) {
        pendingLink = { ...ctx.link, meta: { typeId } };
        const id = await addAt(typeId, pt, true);
        if (id) canvas.completePendingLink(id);
        pendingLink = null;
      } else {
        await addAt(typeId, pt);
      }
    },
  });

  // 3. selection + chrome wiring
  const g = state.graph;
  if (g) {
    const first = g.nodes.find((n) => n.typeId !== "flux.output");
    if (first) state.selection = new Set([first.id]);
  }
  makeTabs();
  renderLibrary();
  renderAll();

  subscribe((evt) => { if (evt === "snapshot") scheduleRender(); });

  document.getElementById("playBtn").onclick = togglePlay;
  document.getElementById("loopBtn").onclick = toggleLoopMode;
  applyLoopButton();
  document.getElementById("fitBtn").onclick = () => canvas.fit();
  document.getElementById("monitorBtn").onclick = () => {
    const open = overlay.toggleMonitor();
    document.getElementById("monitorBtn").classList.toggle("on", open);
    if (open) requestPreviews();
  };
  document.getElementById("monitorClose").onclick = () => {
    overlay.toggleMonitor(false);
    document.getElementById("monitorBtn").classList.remove("on");
  };
  document.getElementById("monitorFull").onclick = () => {
    const img = document.getElementById("monitorImg");
    if (img.requestFullscreen) img.requestFullscreen().catch(() => {});
  };
  document.getElementById("hudFit").onclick = () => canvas.fit();
  document.getElementById("hudAdd").onclick = () => overlay.open({ world: centerWorld() });
  document.getElementById("undoBtn").onclick = () => commands.undo().then(() => { renderAll(); requestPreviews(); });
  document.getElementById("redoBtn").onclick = () => commands.redo().then(() => { renderAll(); requestPreviews(); });
  document.getElementById("newGraphBtn").onclick = () => commands.newGraph("Composition").then(() => { state.selection.clear(); renderAll(); requestPreviews(); });
  document.getElementById("saveBtn").onclick = saveNow;
  document.getElementById("openBtn").onclick = openFile;
  document.getElementById("openFile").addEventListener("change", onOpenFile);
  document.getElementById("exportBtn").onclick = onExport;

  // Export runs on a background thread; progress arrives as events so the
  // preview loop keeps playing while frames are rendered.
  listen("export-progress", (p) => {
    const total = Number(p.total) || 0;
    const done = Number(p.done) || 0;
    const pct = total ? Math.round((done / total) * 100) : 0;
    setStatus(`exporting… ${pct}% (${done}/${total} frames)`);
  });
  listen("export-done", (p) => {
    if (p.video) setStatus(`video → ${p.video}`);
    else if (p.framesDir) setStatus(`frames → ${p.framesDir} (no video produced)`);
    else setStatus(`export done: ${p.frames} frames`);
  });
  listen("export-error", (p) => setStatus("export error: " + (p.message || "unknown"), true));

  const vp = document.getElementById("viewport");
  vp.addEventListener("contextmenu", onContextMenu);

  // drag/drop from the library
  vp.addEventListener("dragover", (ev) => { ev.preventDefault(); ev.dataTransfer.dropEffect = "copy"; });
  vp.addEventListener("drop", async (ev) => {
    ev.preventDefault();
    const world = canvas.toWorld(ev.clientX, ev.clientY);
    const typeId = ev.dataTransfer.getData("application/x-flux-node");
    const presetId = ev.dataTransfer.getData("application/x-flux-preset");
    if (typeId) await addAt(typeId, world);
    else if (presetId) { await commands.insertPreset(presetId, world); state.selection.clear(); renderAll(); requestPreviews(); }
  });

  document.addEventListener("keydown", onKeyDown);
  window.addEventListener("beforeunload", () => persistCanvasMeta());

  // fit on first paint
  requestAnimationFrame(() => { canvas.fit(); requestPreviews(); });

  setStatus("ready · Space to add a node");
}

let pendingLink = null;

window.addEventListener("DOMContentLoaded", boot);
