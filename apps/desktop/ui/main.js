// Flux node-graph editor entry. Boots the engine, wires the view modules, and
// owns the keyboard/transport/preview loops. No engine logic lives here — every
// edit is a command and every frame is rendered by Rust.

import {
  state, subscribe, setCatalog, syncCanvasMeta, manifestOf, selectedNodes,
} from "./store.js";
import * as commands from "./commands.js";
import { renderPreviewRaw, exportProject, listen, buildInfo } from "./ipc.js";
import { initCanvas } from "./view/canvas.js";
import { initLibrary, render as renderLibrary } from "./view/library.js";
import { initInspector, render as renderInspector } from "./view/inspector.js";
import { initTimeline, render as renderTimeline } from "./view/timeline.js";
import { initOverlay } from "./view/overlay.js";
import { nodeRect } from "./view/node.js";
import { portsCompatible } from "./view/cable.js";

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

/** Show measured playback throughput so quality work is verifiable. */
function updatePerf() {
  const el = document.getElementById("perfStat");
  if (!el) return;
  el.textContent = `preview ${perf.fps || 0}fps · ${perf.lastMs}ms`;
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
let previewTimer = null;

// Coalesce parameter-edit re-renders: sliders fire many changes per drag, and
// each render is a full GPU frame. ~40ms keeps the preview live without backing
// the render queue up.
function requestPreviewSoon() {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(() => requestPreviews(), 40);
}

function setThumb(nodeId, w, h, u8) {
  thumbCache.set(nodeId, { w, h, u8 });
  paintThumb(document.querySelector(`canvas[data-thumb-for="${nodeId}"]`), nodeId);
}

function paintThumb(canvas, nodeId) {
  if (!canvas) return;
  const rec = thumbCache.get(nodeId);
  if (!rec) return;
  drawRgba(canvas, rec.w, rec.h, rec.u8);
}

function fillThumb(canvas, nodeId) {
  paintThumb(canvas, nodeId);
}

/** Paint packed RGBA into a canvas (zero-copy view over the received buffer). */
function drawRgba(canvas, w, h, u8) {
  if (!canvas || !w || !h || !u8) return;
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  const ctx = canvas.getContext("2d", { alpha: true });
  const view =
    u8 instanceof Uint8Array
      ? new Uint8ClampedArray(u8.buffer, u8.byteOffset, u8.byteLength)
      : new Uint8ClampedArray(u8);
  ctx.putImageData(new ImageData(view, w, h), 0, 0);
}

// Live playback performance counters, shown in the status bar.
const perf = { frames: 0, lastFpsAt: 0, fps: 0, lastMs: 0 };

function recordPerf(ms) {
  perf.lastMs = Math.round(ms);
  perf.frames++;
  const now = performance.now();
  if (!perf.lastFpsAt) perf.lastFpsAt = now;
  if (now - perf.lastFpsAt >= 1000) {
    perf.fps = Math.round((perf.frames * 1000) / (now - perf.lastFpsAt));
    perf.frames = 0;
    perf.lastFpsAt = now;
  }
}

/** Thumbnail render size: CSS width x devicePixelRatio, capped by the readback cap. */
function thumbSize() {
  const dpr = Math.min(2, globalThis.devicePixelRatio || 1);
  const cssW = 176; // must match NODE_W in view/node.js
  const w = Math.min(960, Math.max(160, Math.round(cssW * dpr)));
  return { w, h: Math.round((w * 9) / 16) };
}

function monitorSize() {
  // The GPU readback is capped at 960x540; render at that for a crisp monitor.
  return { w: 960, h: 540 };
}

function requestPreviews() {
  // While exporting, the export thread owns the GPU and streams its own frames to
  // the monitor. Rendering previews concurrently created a second device and
  // could crash the driver.
  if (state.exporting) return;
  if (previewBusy) { previewPending = true; return; }
  previewBusy = true;
  (async () => {
    try {
      const t0 = performance.now();
      const g = state.graph;
      const outputs = (g?.nodes || []).filter((n) => n.typeId === "flux.output");
      const { w, h } = thumbSize();
      // Render each Output node from raw RGBA: no PNG encode, no base64, no <img>
      // decode — this is what made playback choppy.
      for (const n of outputs) {
        const frame = await renderPreviewRaw(state.activeGraphId, state.time, w, h, n.id);
        setThumb(n.id, frame.w, frame.h, frame.u8);
      }
      if (overlay?.isMonitorOpen()) {
        const ms = monitorSize();
        const frame = await renderPreviewRaw(state.activeGraphId, state.time, ms.w, ms.h, null);
        overlay.setMonitorFrame(frame.w, frame.h, frame.u8);
      }
      recordPerf(performance.now() - t0);
      document.title = `Flux | ok ${w}x${h} ${perf.lastMs}ms ${perf.fps}fps`;
    } catch (e) {
      const msg = String(e?.message || e);
      setStatus("preview: " + msg, true);
      document.title = "Flux | ERR " + msg.slice(0, 90);
    } finally {
      previewBusy = false;
      updatePerf();
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
  try {
    const res = await commands.addNode(typeId, pt.x, pt.y);
    const id = res?.result?.nodeId;
    if (!id) {
      setStatus(`Could not add ${typeId}: the engine returned no node id`, true);
      return null;
    }
    state.selection = new Set([id]);
    if (!skipAutoWire) await autoWire(id, typeId);
    renderAll();
    requestPreviews();
    return id;
  } catch (e) {
    setStatus(`Add node failed: ${e?.message || e}`, true);
    return null;
  }
}

async function insertPresetAt(presetId, pt, mergeInto = state.activeGraphId) {
  try {
    const res = await commands.insertPreset(presetId, pt, mergeInto);
    if (res?.error) throw new Error(`${res.error.code}: ${res.error.message}`);
    state.selection.clear();
    syncCanvasMeta();
    renderAll();
    canvas.fit();
    requestPreviews();
    return res;
  } catch (e) {
    setStatus(`Insert preset failed: ${e?.message || e}`, true);
    return null;
  }
}

/** Wire a newly-created node into the composition sensibly, using matching port
 *  types only:
 *  - a generator with a texture output feeds Output;
 *  - a single-source effect is inserted between the current provider and Output. */
async function autoWire(newId, typeId) {
  const g = state.graph;
  const m = manifestOf(typeId);
  if (!g || !m) return;
  const out = g.nodes.find((n) => n.typeId === "flux.output");
  if (!out) return;

  const outType =
    (manifestOf("flux.output")?.inputs || []).find((p) => p.id === "source")?.type || "texture2d";
  const outEdge = g.edges.find((e) => e.toNode === out.id && e.toPort === "source");
  const provider = outEdge ? outEdge.fromNode : null;

  const outPortOfType = (type, want) =>
    (manifestOf(type)?.outputs || []).find((p) => portsCompatible(p.type, want))?.id || null;

  if (m.kind === "generator") {
    const op = outPortOfType(typeId, outType);
    if (!op) {
      setStatus(`${typeId} has no ${outType} output`, true);
      return;
    }
    if (outEdge) await commands.disconnect(out.id, "source");
    await commands.connect(newId, op, out.id, "source");
    return;
  }

  const srcIn = (m.inputs || []).find((p) => p.id === "source");
  if (!srcIn || !provider || provider === newId) return;
  const provNode = g.nodes.find((n) => n.id === provider);
  const provOut = provNode ? outPortOfType(provNode.typeId, srcIn.type) : null;
  const newOut = outPortOfType(typeId, outType);
  if (!provOut || !newOut) {
    setStatus(`Cannot auto-wire ${typeId}: no matching ports`, true);
    return;
  }
  if (outEdge) await commands.disconnect(out.id, "source");
  await commands.connect(provider, provOut, newId, srcIn.id);
  await commands.connect(newId, newOut, out.id, "source");
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
state.exporting = false;
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

async function startNewComposition() {
  await commands.newGraph("Composition");
  state.selection.clear();
  // Keep a new composition renderable immediately. The reference flow always
  // has a live image branch; an Output-only graph otherwise reports a required
  // input error until the user manually adds a generator.
  await addAt("gen.plasma", { x: 0, y: 0 });
  renderAll();
  canvas.fit();
  requestPreviews();
}

function toggleLoopMode() {
  state.loopMode = state.loopMode === "pingpong" ? "forward" : "pingpong";
  state.playDir = 1;
  try { localStorage.setItem("flux.loopMode", state.loopMode); } catch (e) {}
  applyLoopButton();
}

function playbackTick(dtSeconds) {
  const dur = durationSeconds();
  const dt = dtSeconds;
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

// Playback is driven by requestAnimationFrame and advances by real elapsed time,
// so it runs as fast as frames can actually be produced (the old fixed 100 ms
// interval forced 10 fps regardless of how cheap a frame was).
let playRaf = 0;
let lastTick = 0;
function playbackLoop(now) {
  if (!state.playing) return;
  if (!lastTick) lastTick = now;
  const dt = Math.min(0.25, (now - lastTick) / 1000);
  lastTick = now;
  playbackTick(dt);
  playRaf = requestAnimationFrame(playbackLoop);
}

function startPlayback() {
  if (state.playing) return;
  state.playing = true;
  const btn = document.getElementById("playBtn");
  btn.textContent = "Pause";
  btn.classList.add("on");
  lastTick = 0;
  playRaf = requestAnimationFrame(playbackLoop);
}

function stopPlayback() {
  state.playing = false;
  cancelAnimationFrame(playRaf);
  playRaf = 0;
  lastTick = 0;
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

  state.exporting = true;
  setStatus(`exporting ${codec.toUpperCase()}…`);
  try {
    const msg = await exportProject(outDir, w, h, codec, keepFrames);
    if (msg === "export started") setStatus("exporting… rendering frames");
    else setStatus(String(msg));
  } catch (e) {
    state.exporting = false;
    requestPreviews();
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
  window.__fluxBooted = true;
  commands.setStatusSink((m) => setStatus(m, true));
  setStatus("starting Flux…");

  // Show which build is running — makes a stale binary obvious immediately.
  buildInfo().then((info) => {
    const el = document.getElementById("buildStat");
    if (el && info) el.textContent = `build ${info.gitHash || "?"} v${info.version || ""}`;
  });

  // 1. project + catalog
  await commands.createProject("Untitled", 1, 1280, 720, 60, 1);
  const cat = await commands.catalog();
  if (cat?.result?.entries) setCatalog(cat.result.entries);

  const presets = await commands.listPresets();
  if (presets?.result?.presets) state.presets = presets.result.presets;

  // Start with the closest built-in composition to the captured reference:
  // generator -> chromatic treatment -> VHS -> CRT -> output. This keeps the
  // editor useful on first launch while leaving every node fully editable.
  if (state.presets.some((p) => p.id === "preset.vhs_tracking")) {
    await commands.insertPreset("preset.vhs_tracking", { x: 0, y: 0 });
  }

  syncCanvasMeta();

  // 2. views
  canvas = initCanvas({
    store: { state },
    commands: { ...commands, replaceConnect },
    manifestOf,
    fillThumb,
    onParamChanged: () => requestPreviewSoon(),
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
    onInsertPreset: (presetId, pt) => insertPresetAt(presetId, pt || centerWorld()),
  });

  inspector = initInspector({
    commands,
    onFocusNode: (id) => canvas.focusNode(id),
    // Every parameter edit must re-render: the engine stores the value, but the
    // image only changes when a new frame is requested.
    onParamChanged: () => requestPreviewSoon(),
    onGraphChanged: () => { scheduleRender(); requestPreviews(); },
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
    commands.openMonitor()
      .then(() => document.getElementById("monitorBtn").classList.add("on"))
      .catch((e) => setStatus("monitor: " + e, true));
  };
  document.getElementById("monitorClose").onclick = () => {
    overlay.toggleMonitor(false);
    document.getElementById("monitorBtn").classList.remove("on");
  };
  document.getElementById("monitorFull").onclick = () => {
    commands.toggleMonitorFullscreen().catch((e) => setStatus("monitor: " + e, true));
  };
  document.getElementById("hudFit").onclick = () => canvas.fit();
  document.getElementById("hudAdd").onclick = () => overlay.open({ world: centerWorld() });
  document.getElementById("undoBtn").onclick = () => commands.undo().then(() => { renderAll(); requestPreviews(); });
  document.getElementById("redoBtn").onclick = () => commands.redo().then(() => { renderAll(); requestPreviews(); });
  document.getElementById("newGraphBtn").onclick = startNewComposition;
  document.getElementById("importMediaBtn").onclick = () => document.getElementById("mediaFile").click();
  document.getElementById("mediaFile").addEventListener("change", async (ev) => {
    const file = ev.target.files?.[0];
    if (!file) return;
    const path = file.path || file.webkitRelativePath;
    if (!path || path === file.name) {
      setStatus("media import needs a desktop file path", true);
      return;
    }
    const res = await commands.importMedia(path);
    if (!res?.error) {
      state.selection.clear();
      renderAll();
      canvas.fit();
      requestPreviews();
      setStatus(`loaded ${file.name}`);
    }
    ev.target.value = "";
  });
  document.getElementById("saveBtn").onclick = saveNow;
  document.getElementById("openBtn").onclick = openFile;
  document.getElementById("openFile").addEventListener("change", onOpenFile);
  document.getElementById("exportBtn").onclick = onExport;
  for (const id of ["expW", "expH"]) {
    const input = document.getElementById(id);
    const output = document.getElementById(id + "Val");
    input?.addEventListener("input", () => { if (output) output.textContent = input.value; });
  }

  // Export runs on a background thread; the monitor shows the real rendered
  // output frames while the composition is processed.
  listen("export-frame", (p) => {
    if (!overlay.isMonitorOpen()) {
      overlay.toggleMonitor(true);
      document.getElementById("monitorBtn").classList.add("on");
    }
    if (p?.dataUrl) overlay.setMonitorImage(p.dataUrl);
  });
  listen("export-progress", (p) => {
    const total = Number(p.total) || 0;
    const done = Number(p.done) || 0;
    const pct = total ? Math.round((done / total) * 100) : 0;
    setStatus(`exporting… ${pct}% (${done}/${total} frames)`);
  });
  listen("export-done", (p) => {
    state.exporting = false;
    if (p.video) setStatus(`video → ${p.video}`);
    else if (p.framesDir) setStatus(`frames → ${p.framesDir} (no video produced)`);
    else setStatus(`export done: ${p.frames} frames`);
    requestPreviews();
  });
  listen("export-error", (p) => {
    state.exporting = false;
    setStatus("export error: " + (p.message || "unknown"), true);
    requestPreviews();
  });

  // Live control arrives from Rust, not from a click. Without this the preview
  // moves (it re-reads the graph from Rust every frame) while the inspector keep
  // showing the value the fader replaced, so the two halves of one edit disagree.
  // Rust stays the authority: this only re-pulls the snapshot.
  //
  // One pull at a time: commands are serialised in the IPC layer, so a 60 Hz
  // stream of external edits queued unbounded would put the operator's own
  // clicks behind the fader that caused them.
  let pulling = false;
  const pullSnapshot = () => {
    if (pulling) return;
    pulling = true;
    commands
      .snapshot()
      .catch(() => {})
      .finally(() => {
        pulling = false;
      });
  };
  listen("live-control", pullSnapshot);

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
    else if (presetId) await insertPresetAt(presetId, world);
  });

  document.addEventListener("keydown", onKeyDown);
  window.addEventListener("beforeunload", () => persistCanvasMeta());

  // fit on first paint
  requestAnimationFrame(() => { canvas.fit(); requestPreviews(); });

  setStatus(
    `ready · ${state.graph?.nodes?.length || 0} nodes · ${state.catalog.length} node types · ${state.presets.length} presets`
  );
}

let pendingLink = null;

window.addEventListener("DOMContentLoaded", () => {
  boot().catch((e) => {
    const message = e?.message || String(e);
    console.error("Flux startup failed", e);
    setStatus("startup failed: " + message, true);
    document.title = "Flux | startup error";
  });
});
