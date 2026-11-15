(() => {
  var __defProp = Object.defineProperty;
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };

  // store.js
  var state = {
    revision: 0,
    project: null,
    graph: null,
    // active graph object { id, nodes, edges, outputNode }
    activeGraphId: null,
    catalog: [],
    catalogById: /* @__PURE__ */ new Map(),
    selection: /* @__PURE__ */ new Set(),
    // node ids
    selectedClip: null,
    time: 0,
    playing: false,
    view: { x: 0, y: 0, zoom: 1 },
    frames: [],
    // [{ id, title, rect:[x,y,w,h] }]
    presets: [],
    ui: { libTab: "nodes", collapsed: {}, inspector: true, minimap: true, perf: true }
  };
  var listeners = /* @__PURE__ */ new Set();
  function subscribe(fn) {
    listeners.add(fn);
    return () => listeners.delete(fn);
  }
  function emit(evt = "change") {
    for (const fn of listeners) fn(evt);
  }
  function setCatalog(entries) {
    state.catalog = entries;
    state.catalogById = new Map(entries.map((e) => [e.id, e]));
  }
  function manifestOf(typeId) {
    return state.catalogById.get(typeId);
  }
  function applyResponse(res) {
    if (!res) return;
    if (res.revision != null) state.revision = res.revision;
    const r = res.result || {};
    if (r.project && r.project.projectId) state.project = r.project;
    if (r.activeGraphId) state.activeGraphId = r.activeGraphId;
    if (r.activeGraph !== void 0) state.graph = r.activeGraph;
    emit("snapshot");
  }
  function syncCanvasMeta() {
    const ext = state.project?.extensions?.["dev.flux.canvas"];
    state.frames = Array.isArray(ext?.frames) ? ext.frames : [];
    if (ext?.viewport) state.view = { ...state.view, ...ext.viewport };
  }
  function selectedNodes() {
    if (!state.graph) return [];
    return state.graph.nodes.filter((n) => state.selection.has(n.id));
  }

  // commands.js
  var commands_exports = {};
  __export(commands_exports, {
    addNode: () => addNode,
    addNodeObj: () => addNodeObj,
    catalog: () => catalog,
    connect: () => connect,
    createProject: () => createProject,
    disconnect: () => disconnect,
    importMedia: () => importMedia,
    insertPreset: () => insertPreset,
    listPresets: () => listPresets,
    loadProject: () => loadProject,
    moveNode: () => moveNode,
    moveNodes: () => moveNodes,
    newGraph: () => newGraph,
    openMonitor: () => openMonitor,
    paramValue: () => paramValue,
    redo: () => redo,
    removeMapping: () => removeMapping,
    removeNode: () => removeNode,
    removeNodes: () => removeNodes,
    saveProject: () => saveProject,
    setBypass: () => setBypass,
    setCanvasMeta: () => setCanvasMeta,
    setGraphOutput: () => setGraphOutput,
    setKeyframes: () => setKeyframes,
    setMapping: () => setMapping,
    setParam: () => setParam,
    setParamLive: () => setParamLive,
    setStatusSink: () => setStatusSink,
    snapshot: () => snapshot,
    toggleMonitorFullscreen: () => toggleMonitorFullscreen,
    undo: () => undo
  });

  // ipc.js
  var invoke = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
  if (typeof invoke !== "function") {
    throw new Error("Tauri IPC bridge is unavailable");
  }
  function rid() {
    try {
      if (crypto && typeof crypto.randomUUID === "function") return crypto.randomUUID();
    } catch (e) {
    }
    return "r-" + Math.random().toString(36).slice(2) + Date.now().toString(36);
  }
  async function dispatch(command, expectedRevision) {
    const req = {
      protocolVersion: 1,
      requestId: rid(),
      projectId: null,
      expectedRevision: expectedRevision ?? 0,
      command
    };
    return invoke("dispatch", { request: req });
  }
  async function renderPreviewRaw(graphId, timeSeconds, width, height, outputNodeId) {
    const buf = await invoke("render_preview_raw", {
      graphId,
      timeSeconds,
      width,
      height,
      outputNodeId: outputNodeId ?? null
    });
    const u8 = buf instanceof Uint8Array ? buf : buf instanceof ArrayBuffer ? new Uint8Array(buf) : Uint8Array.from(buf);
    return { w: width, h: height, u8 };
  }
  async function buildInfo() {
    try {
      return await invoke("build_info");
    } catch (e) {
      return null;
    }
  }
  async function exportProject(outDir, width, height, codec = "h264", keepFrames = false) {
    return invoke("export_project_cmd", { outDir, width, height, codec, keepFrames });
  }
  function listen(event, handler) {
    const ev = window.__TAURI__?.event;
    if (!ev || !ev.listen) return Promise.resolve(() => {
    });
    return ev.listen(event, (e) => handler(e.payload));
  }

  // commands.js
  var onStatus = (m) => console.warn(m);
  function setStatusSink(fn) {
    onStatus = fn;
  }
  var inFlight = Promise.resolve();
  function enqueue(task) {
    const run2 = inFlight.then(task, task);
    inFlight = run2.catch(() => {
    });
    return run2;
  }
  async function run(command, { expected = state.revision } = {}) {
    const res = await dispatch(command, expected);
    if (res.error) {
      if (res.error.code === "RevisionConflict") {
        const snap = await dispatch({ type: "snapshot" }, 0);
        applyResponse(snap);
        const retry = await dispatch(command, state.revision);
        if (retry.error) {
          onStatus(`${retry.error.code}: ${retry.error.message}`);
          return retry;
        }
        applyResponse(retry);
        await refreshIfNeeded(retry);
        return retry;
      }
      onStatus(`${res.error.code}: ${res.error.message}`);
      throw new Error(`${res.error.code}: ${res.error.message}`);
    }
    applyResponse(res);
    await refreshIfNeeded(res);
    syncCanvasMeta();
    return res;
  }
  async function refreshIfNeeded(res) {
    const r = res?.result;
    const hasSnapshot = r && (r.project || r.activeGraph !== void 0);
    if (hasSnapshot) return;
    const snap = await dispatch({ type: "snapshot" }, 0);
    if (!snap.error) {
      applyResponse(snap);
      syncCanvasMeta();
    }
  }
  function addNode(typeId, x, y) {
    return enqueue(() => run({ type: "addGraphNode", typeId, x, y }));
  }
  function addNodeObj(node) {
    return enqueue(
      () => run({
        type: "apply",
        command: { type: "addNode", graphId: state.activeGraphId, node }
      })
    );
  }
  function removeNode(nodeId) {
    return enqueue(() => run({ type: "removeGraphNode", nodeId }));
  }
  function removeNodes(nodeIds) {
    return enqueue(async () => {
      let last;
      for (const id of nodeIds) last = await run({ type: "removeGraphNode", nodeId: id });
      return last;
    });
  }
  function connect(fromNode, fromPort, toNode, toPort) {
    return enqueue(() => run({ type: "connectPins", fromNode, fromPort, toNode, toPort }));
  }
  function disconnect(toNode, toPort) {
    return enqueue(() => run({ type: "disconnectPin", toNode, toPort }));
  }
  function moveNode(nodeId, x, y) {
    return enqueue(() => run({ type: "moveNode", nodeId, x, y }));
  }
  function moveNodes(list) {
    return enqueue(async () => {
      let last;
      for (const n of list) last = await run({ type: "moveNode", nodeId: n.nodeId, x: n.x, y: n.y });
      return last;
    });
  }
  function setBypass(nodeId, bypass) {
    return enqueue(() => run({ type: "setBypass", nodeId, bypass }));
  }
  function setGraphOutput(nodeId) {
    return enqueue(() => run({ type: "setGraphOutput", nodeId }));
  }
  function newGraph(name) {
    return enqueue(() => run({ type: "newGraph", name }));
  }
  function importMedia(path) {
    return enqueue(() => run({ type: "importMedia", path }));
  }
  function openMonitor() {
    const invoke2 = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
    return invoke2("open_monitor");
  }
  function toggleMonitorFullscreen() {
    const invoke2 = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
    return invoke2("toggle_monitor_fullscreen");
  }
  function paramValue(kind, value) {
    if (kind === "bool") return { kind: "bool", value: !!value };
    if (kind === "int" || kind === "enum") return { kind: "int", value: Math.round(Number(value) || 0) };
    if (kind === "color") return { kind: "color", value };
    if (kind === "vec2") return { kind: "vec2", value };
    if (kind === "vec3") return { kind: "vec3", value };
    if (kind === "text") return { kind: "text", value: String(value ?? "") };
    return { kind: "float", value: Number(value) || 0 };
  }
  function setParam(nodeId, paramId, kind, value) {
    const pv = paramValue(kind, value);
    return enqueue(
      () => run({
        type: "apply",
        command: { type: "setParameter", graphId: state.activeGraphId, nodeId, paramId, value: pv }
      })
    );
  }
  var pending = /* @__PURE__ */ new Map();
  var flushScheduled = false;
  function setParamLive(nodeId, paramId, kind, value) {
    pending.set(`${nodeId}:${paramId}`, { nodeId, paramId, kind, value });
    if (!flushScheduled) {
      flushScheduled = true;
      requestAnimationFrame(() => {
        flushScheduled = false;
        const batch = [...pending.values()];
        pending.clear();
        for (const p of batch) setParam(p.nodeId, p.paramId, p.kind, p.value);
      });
    }
  }
  function setKeyframes(laneId, keyframes) {
    return enqueue(() => run({ type: "setKeyframes", laneId, keyframes }));
  }
  function setMapping(mapping) {
    return enqueue(() => run({ type: "setMapping", mapping }));
  }
  function removeMapping(mappingId) {
    return enqueue(() => run({ type: "removeMapping", mappingId }));
  }
  function listPresets() {
    return enqueue(() => run({ type: "presets" }, { expected: 0 }));
  }
  function insertPreset(presetId, position, mergeInto = null) {
    const pos = Array.isArray(position) ? position : [Number(position?.x) || 0, Number(position?.y) || 0];
    return enqueue(
      () => run({ type: "insertPreset", presetId, position: pos, mergeInto })
    );
  }
  function undo() {
    return enqueue(() => run({ type: "undo" }));
  }
  function redo() {
    return enqueue(() => run({ type: "redo" }));
  }
  function snapshot() {
    return enqueue(() => run({ type: "snapshot" }, { expected: 0 }));
  }
  function catalog() {
    return enqueue(() => run({ type: "catalog" }, { expected: 0 }));
  }
  function createProject(name, seed, width, height, fpsNum, fpsDen) {
    return enqueue(
      () => run({ type: "createProject", name, seed, width, height, fpsNum, fpsDen }, { expected: 0 })
    );
  }
  function saveProject(path) {
    return enqueue(() => run({ type: "save", path }));
  }
  function loadProject(json) {
    return enqueue(() => run({ type: "loadProject", json }, { expected: 0 }));
  }
  function setCanvasMeta(frames, viewport) {
    return enqueue(() => run({ type: "setCanvasMeta", frames, viewport }));
  }

  // view/cable.js
  function bezier(a, b) {
    const dx = Math.max(46, Math.abs(b.x - a.x) * 0.5);
    return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
  }
  var PORT_TYPES = {
    texture2d: { color: "#6fb1ff", w: 2.5 },
    mask: { color: "#f0b429", w: 2 },
    depth: { color: "#8ae6c8", w: 2 },
    normal: { color: "#c58bff", w: 2 },
    color: { color: "#ff9f6b", w: 2 },
    scalar: { color: "#9ad0ff", w: 1.6 },
    vector_field: { color: "#c0e06a", w: 1.6 },
    path_set: { color: "#e6a8ff", w: 1.6 }
  };
  function portStyle(type) {
    return PORT_TYPES[type] || { color: "var(--cable)", w: 2 };
  }
  function portsCompatible(fromType, toType) {
    if (!fromType || !toType) return false;
    if (fromType === toType) return true;
    const imagey = /* @__PURE__ */ new Set(["texture2d", "mask"]);
    return imagey.has(fromType) && imagey.has(toType);
  }

  // view/node.js
  var NODE_W = 176;
  var HDR_H = 28;
  var ROW_H = 20;
  var CAT_COLORS = {
    Generators: "#4a9eff",
    Vector: "#5f8dff",
    Typography: "#8f7bff",
    "Image animation": "#4fb3d9",
    "3D SDF": "#7a9cff",
    "3D Mesh": "#6f8ff0",
    Particles: "#c58bff",
    Physics: "#b07bff",
    Simulation: "#7bd0c0",
    Control: "#e0c060",
    Conditional: "#d98b6b",
    Blur: "#5fb4c9",
    Color: "#f0a060",
    Blend: "#c08bff",
    Distortion: "#ff8f6b",
    Light: "#ffd166",
    Optical: "#8fd0e0",
    Material: "#b8b8c8",
    Glitch: "#ff6b8b",
    Analog: "#e07b9a",
    Temporal: "#9ad0ff",
    Matte: "#b0b8c4",
    Utility: "#9aa4b2",
    "Input/Output": "#46c07a",
    Mask: "#f0b429",
    "Print and Pixel": "#c9a46b",
    Style: "#a0b0c0",
    Texture: "#9ab0c0",
    Technical: "#8fa0b0"
  };
  function categoryColor(category) {
    return CAT_COLORS[category] || "#6fb1ff";
  }
  function portClass(port) {
    if (port.type === "mask" || port.id === "mask") return "mask";
    if (port.id === "b") return "aux";
    return "";
  }
  function inlineParams(manifest) {
    const params = (manifest?.parameters || []).filter(
      (p) => (p.kind === "float" || p.kind === "int" || p.kind === "bool") && !p.hidden
    );
    return params.slice(0, 2);
  }
  function planLayout(node, manifest) {
    const inputs = manifest?.inputs || [];
    const outputs = manifest?.outputs || [];
    const rows = Math.max(inputs.length, outputs.length, 1);
    const portsH = rows * ROW_H + 8;
    const inline = inlineParams(manifest);
    const inlineH = inline.length ? inline.length * 22 + 8 : 0;
    const isOutput = node.typeId === "flux.output";
    const thumbH = isOutput ? Math.round(NODE_W * 9 / 16) + 6 : 0;
    const h = HDR_H + portsH + inlineH + thumbH;
    return { inputs, outputs, rows, portsH, inline, inlineH, thumbH, h };
  }
  function portY(plan, index) {
    return HDR_H + 4 + index * ROW_H + ROW_H / 2;
  }
  function portPoint(node, manifest, dir, portId) {
    const plan = planLayout(node, manifest);
    const ports = dir === "in" ? plan.inputs : plan.outputs;
    const idx = Math.max(0, ports.findIndex((p) => p.id === portId));
    const x = node.position[0] + (dir === "in" ? 0 : NODE_W);
    const y = node.position[1] + portY(plan, idx);
    return { x, y };
  }
  function nodeRect(node, manifest) {
    const plan = planLayout(node, manifest);
    return { x: node.position[0], y: node.position[1], w: NODE_W, h: plan.h };
  }
  function createNodeEl(node, opts5) {
    const m = opts5.manifestOf(node.typeId);
    const plan = planLayout(node, m);
    const el3 = document.createElement("div");
    el3.className = "node" + (opts5.isSelected(node.id) ? " sel" : "") + (node.bypass ? " bypassed" : "") + (node.missingType ? " missing" : "");
    el3.style.left = node.position[0] + "px";
    el3.style.top = node.position[1] + "px";
    el3.dataset.nodeId = node.id;
    el3.style.setProperty("--cat", categoryColor(m?.category || "Utility"));
    const hdr = document.createElement("div");
    hdr.className = "hdr";
    const dot = document.createElement("span");
    dot.className = "catdot";
    const nm = document.createElement("span");
    nm.className = "nm";
    nm.textContent = m?.name ?? node.typeId;
    hdr.appendChild(dot);
    hdr.appendChild(nm);
    if (node.bypass) {
      const b = document.createElement("span");
      b.className = "byp";
      b.textContent = "BYP";
      hdr.appendChild(b);
    }
    hdr.title = `${m?.name ?? node.typeId}${m?.help ? " \u2014 " + m.help : ""}`;
    hdr.addEventListener("pointerdown", (ev) => opts5.callbacks.onHeaderDown(ev, node));
    el3.appendChild(hdr);
    const ports = document.createElement("div");
    ports.className = "ports";
    for (let i = 0; i < plan.rows; i++) {
      const row = document.createElement("div");
      row.className = "prow";
      if (plan.inputs[i]) {
        const p = plan.inputs[i];
        row.classList.add("in");
        const d = document.createElement("div");
        d.className = "port in " + portClass(p);
        if (opts5.isConnected(node.id, p.id, "in")) d.classList.add("connected");
        d.dataset.node = node.id;
        d.dataset.port = p.id;
        d.dataset.dir = "in";
        d.dataset.ptype = p.type;
        d.title = `input ${p.id} (${p.type})${p.required ? " \xB7 required" : ""}`;
        d.addEventListener("pointerdown", (ev) => opts5.callbacks.onPortDown(ev, node, p, "in"));
        d.addEventListener("pointerup", (ev) => opts5.callbacks.onPortUp(ev, node, p, "in"));
        row.appendChild(d);
        const lbl = document.createElement("span");
        lbl.className = "plbl";
        lbl.textContent = p.id;
        row.appendChild(lbl);
      }
      if (plan.outputs[i]) {
        const p = plan.outputs[i];
        row.classList.add("out");
        const lbl = document.createElement("span");
        lbl.className = "plbl";
        lbl.textContent = p.id;
        row.appendChild(lbl);
        const d = document.createElement("div");
        d.className = "port out " + portClass(p);
        if (opts5.isConnected(node.id, p.id, "out")) d.classList.add("connected");
        d.dataset.node = node.id;
        d.dataset.port = p.id;
        d.dataset.dir = "out";
        d.dataset.ptype = p.type;
        d.title = `output ${p.id} (${p.type})`;
        d.addEventListener("pointerdown", (ev) => opts5.callbacks.onPortDown(ev, node, p, "out"));
        d.addEventListener("pointerup", (ev) => opts5.callbacks.onPortUp(ev, node, p, "out"));
        row.appendChild(d);
      }
      ports.appendChild(row);
    }
    el3.appendChild(ports);
    if (plan.inline.length) {
      const box = document.createElement("div");
      box.className = "inline";
      for (const decl of plan.inline) {
        const val = opts5.paramValue(node, decl);
        const wrap = document.createElement("div");
        wrap.className = "ival";
        const lbl = document.createElement("label");
        lbl.textContent = decl.label || decl.id;
        lbl.title = decl.label || decl.id;
        wrap.appendChild(lbl);
        const input = document.createElement("input");
        input.type = decl.kind === "bool" ? "checkbox" : "range";
        if (decl.kind === "bool") input.checked = !!val;
        else {
          const min = Number.isFinite(Number(decl.min)) ? Number(decl.min) : Math.min(0, Number(val) || 0);
          const max = Number.isFinite(Number(decl.max)) ? Number(decl.max) : Math.max(1, Number(val) || 0, min + 1);
          input.min = min;
          input.max = max;
          input.value = val;
          input.step = decl.kind === "int" ? "1" : "0.01";
        }
        input.addEventListener("pointerdown", (ev) => ev.stopPropagation());
        input.addEventListener("input", () => {
          const v = decl.kind === "bool" ? input.checked : Number(input.value);
          opts5.callbacks.onInlineParam(node.id, decl.id, decl.kind, v);
        });
        wrap.appendChild(input);
        box.appendChild(wrap);
      }
      el3.appendChild(box);
    }
    if (node.typeId === "flux.output") {
      const canvas2 = document.createElement("canvas");
      canvas2.className = "thumb";
      canvas2.dataset.thumbFor = node.id;
      const cssW = NODE_W;
      const dpr = Math.min(2, globalThis.devicePixelRatio || 1);
      canvas2.width = Math.round(cssW * dpr);
      canvas2.height = Math.round(cssW * 9 / 16 * dpr);
      canvas2.addEventListener("pointerdown", (ev) => ev.stopPropagation());
      el3.appendChild(canvas2);
    }
    el3.addEventListener("pointerdown", (ev) => {
      if (ev.target.classList.contains("port")) return;
      opts5.callbacks.onNodeDown(ev, node);
    });
    return el3;
  }

  // view/canvas.js
  var MIN_ZOOM = 0.2;
  var MAX_ZOOM = 2.2;
  function initCanvas(opts5) {
    const viewport = document.getElementById("viewport");
    const world = document.getElementById("world");
    const svg = document.getElementById("edges");
    const edgeG = document.getElementById("edgeLayer");
    const marquee = document.getElementById("marquee");
    const minimap = document.getElementById("minimap");
    const nodeEls = /* @__PURE__ */ new Map();
    let view = opts5.store.state.view;
    let drag = null;
    let pan = null;
    let linking = null;
    let marq = null;
    const store = opts5.store;
    const graph = () => store.state.graph;
    function viewportRect() {
      return viewport.getBoundingClientRect();
    }
    function toWorld(clientX, clientY) {
      const r = viewportRect();
      return { x: view.x + (clientX - r.left) / view.zoom, y: view.y + (clientY - r.top) / view.zoom };
    }
    function applyTransform() {
      world.style.transform = `translate(${-view.x * view.zoom}px, ${-view.y * view.zoom}px) scale(${view.zoom})`;
    }
    function setView(v) {
      view = v;
      store.state.view = v;
      applyTransform();
      drawMinimap();
    }
    function edgeType(fromNode, fromPort) {
      const m = opts5.manifestOf(fromNode.typeId);
      const p = (m?.outputs || []).find((x) => x.id === fromPort);
      return p?.type;
    }
    function renderEdges() {
      const g = graph();
      edgeG.innerHTML = "";
      if (!g) return;
      const flow = g.nodes.some((n) => {
        const m = opts5.manifestOf(n.typeId);
        return m?.category === "Temporal" || m?.generator === false && n.typeId.includes("feedback");
      });
      g.edges.forEach((e, idx) => {
        const from = g.nodes.find((n) => n.id === e.fromNode);
        const to = g.nodes.find((n) => n.id === e.toNode);
        if (!from || !to) return;
        const a = portPoint(from, opts5.manifestOf(from.typeId), "out", e.fromPort);
        const b = portPoint(to, opts5.manifestOf(to.typeId), "in", e.toPort);
        const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
        let cls = "edge";
        if (from.bypass) cls += " bypassed";
        if (flow) cls += " flow";
        path.setAttribute("class", cls);
        const st = portStyle(edgeType(from, e.fromPort));
        path.style.stroke = st.color;
        path.style.strokeWidth = st.w;
        path.setAttribute("d", bezier(a, b));
        path.dataset.edge = idx;
        path.addEventListener("pointerdown", (ev) => ev.stopPropagation());
        path.addEventListener("click", (ev) => {
          ev.stopPropagation();
          opts5.onEdgeClick(e);
        });
        path.addEventListener("dblclick", (ev) => {
          ev.stopPropagation();
          opts5.onEdgeDoubleClick(e, idx);
        });
        edgeG.appendChild(path);
      });
    }
    function nodeSignature(node) {
      const m = opts5.manifestOf(node.typeId);
      const conn = (graph()?.edges || []).filter((e) => e.fromNode === node.id || e.toNode === node.id).map((e) => `${e.fromNode}:${e.fromPort}>${e.toNode}:${e.toPort}`).join("|");
      const inlineVals = (m?.parameters || []).filter((p) => p.kind === "float" || p.kind === "int" || p.kind === "bool").slice(0, 2).map((p) => {
        const ov = node.params?.[p.id];
        return `${p.id}=${ov !== void 0 ? JSON.stringify(ov.value) : JSON.stringify(p.default)}`;
      }).join(",");
      return [
        node.typeId,
        node.bypass ? 1 : 0,
        node.missingType ? 1 : 0,
        store.state.selection.has(node.id) ? 1 : 0,
        conn,
        inlineVals,
        m ? 1 : 0
      ].join("#");
    }
    const cb = {
      manifestOf: opts5.manifestOf,
      isSelected: (id) => store.state.selection.has(id),
      isConnected: (nodeId, portId, dir) => (graph()?.edges || []).some(
        (e) => dir === "in" ? e.toNode === nodeId && e.toPort === portId : e.fromNode === nodeId && e.fromPort === portId
      ),
      paramValue: (node, decl) => {
        const ov = node.params?.[decl.id];
        return ov !== void 0 ? ov.value : decl.default;
      },
      callbacks: {
        onHeaderDown: (ev, node) => startNodeDrag(ev, node),
        onNodeDown: (ev, node) => selectNode(ev, node),
        onPortDown: (ev, node, port, dir) => startLink(ev, node, port, dir),
        // Connection completion is handled by the window-level pointerup in
        // onLinkUp (elementFromPoint resolves the target port), so the per-port
        // handler is intentionally a no-op.
        onPortUp: () => {
        },
        onInlineParam: (nodeId, paramId, kind, value) => {
          opts5.commands.setParamLive(nodeId, paramId, kind, value);
          opts5.onParamChanged?.();
        }
      }
    };
    function renderNodes() {
      const g = graph();
      if (!g) return;
      const seen = /* @__PURE__ */ new Set();
      for (const node of g.nodes) {
        seen.add(node.id);
        const sig = nodeSignature(node);
        const existing = nodeEls.get(node.id);
        if (existing && existing.sig === sig) {
          existing.el.style.left = node.position[0] + "px";
          existing.el.style.top = node.position[1] + "px";
          continue;
        }
        const el3 = createNodeEl(node, cb);
        if (existing) existing.el.replaceWith(el3);
        else world.appendChild(el3);
        nodeEls.set(node.id, { el: el3, sig });
      }
      for (const [id, rec] of nodeEls) {
        if (!seen.has(id)) {
          rec.el.remove();
          nodeEls.delete(id);
        }
      }
      updateThumbs();
    }
    function updateThumbs() {
      const g = graph();
      if (!g) return;
      for (const node of g.nodes) {
        if (node.typeId !== "flux.output") continue;
        const el3 = nodeEls.get(node.id)?.el;
        const cv = el3?.querySelector("canvas.thumb");
        if (cv) opts5.fillThumb(cv, node.id);
      }
    }
    function updatePositions(ids) {
      const g = graph();
      if (!g) return;
      const set = ids ? new Set(ids) : null;
      for (const node of g.nodes) {
        if (set && !set.has(node.id)) continue;
        const rec = nodeEls.get(node.id);
        if (rec) {
          rec.el.style.left = node.position[0] + "px";
          rec.el.style.top = node.position[1] + "px";
        }
      }
      renderEdges();
      drawMinimap();
    }
    function renderFrames() {
      for (const el3 of [...world.querySelectorAll(".frame")]) el3.remove();
      for (const f of store.state.frames || []) {
        const d = document.createElement("div");
        d.className = "frame";
        const [x, y, w, h] = f.rect || [0, 0, 200, 160];
        d.style.left = x + "px";
        d.style.top = y + "px";
        d.style.width = w + "px";
        d.style.height = h + "px";
        if (f.color) d.style.borderColor = f.color;
        const t = document.createElement("div");
        t.className = "ftitle";
        t.textContent = f.title || "Frame";
        d.appendChild(t);
        world.appendChild(d);
      }
    }
    function render4() {
      applyTransform();
      renderFrames();
      renderEdges();
      renderNodes();
      drawMinimap();
    }
    function selectNode(ev, node) {
      const s = store.state.selection;
      if (ev.ctrlKey || ev.metaKey) {
        selectAllConnected(node.id);
        return;
      }
      if (ev.shiftKey) {
        if (s.has(node.id)) s.delete(node.id);
        else s.add(node.id);
      } else if (!s.has(node.id)) {
        s.clear();
        s.add(node.id);
      }
      opts5.onSelectionChanged();
    }
    function selectAllConnected(nodeId) {
      const g = graph();
      if (!g) return;
      const seen = /* @__PURE__ */ new Set([nodeId]);
      const queue = [nodeId];
      while (queue.length) {
        const id = queue.pop();
        for (const e of g.edges) {
          if (e.fromNode === id && !seen.has(e.toNode)) {
            seen.add(e.toNode);
            queue.push(e.toNode);
          }
          if (e.toNode === id && !seen.has(e.fromNode)) {
            seen.add(e.fromNode);
            queue.push(e.fromNode);
          }
        }
      }
      store.state.selection = seen;
      opts5.onSelectionChanged();
    }
    function startNodeDrag(ev, node) {
      if (ev.button !== 0) return;
      ev.stopPropagation();
      ev.preventDefault();
      const s = store.state.selection;
      if (!s.has(node.id)) {
        s.clear();
        s.add(node.id);
        opts5.onSelectionChanged();
      }
      if (ev.altKey) {
        opts5.onDuplicateSelection(Array.from(s));
        return;
      }
      const g = graph();
      if (!g) return;
      const starts = /* @__PURE__ */ new Map();
      const ids = [];
      for (const id of s) {
        const n = g.nodes.find((x) => x.id === id);
        if (n) {
          starts.set(id, [n.position[0], n.position[1]]);
          ids.push(id);
        }
      }
      drag = { ids, starts, originWorld: toWorld(ev.clientX, ev.clientY), moved: false };
      window.addEventListener("pointermove", onDragMove);
      window.addEventListener("pointerup", onDragEnd);
    }
    function onDragMove(ev) {
      if (!drag) return;
      const w = toWorld(ev.clientX, ev.clientY);
      const dx = w.x - drag.originWorld.x;
      const dy = w.y - drag.originWorld.y;
      if (Math.abs(dx) + Math.abs(dy) > 2) drag.moved = true;
      const g = graph();
      if (!g) return;
      for (const id of drag.ids) {
        const n = g.nodes.find((x) => x.id === id);
        const st = drag.starts.get(id);
        if (n && st) n.position = [st[0] + dx, st[1] + dy];
      }
      updatePositions(drag.ids);
    }
    function onDragEnd() {
      window.removeEventListener("pointermove", onDragMove);
      window.removeEventListener("pointerup", onDragEnd);
      if (!drag) return;
      const g = graph();
      if (drag.moved && g) {
        const list = drag.ids.map((id) => {
          const n = g.nodes.find((x) => x.id === id);
          return n ? { nodeId: id, x: n.position[0], y: n.position[1] } : null;
        }).filter(Boolean);
        if (list.length) opts5.commands.moveNodes(list);
      }
      drag = null;
    }
    function startLink(ev, node, port, dir) {
      ev.stopPropagation();
      ev.preventDefault();
      if (ev.button !== 0) return;
      const tmp = document.createElementNS("http://www.w3.org/2000/svg", "path");
      tmp.setAttribute("class", "temp");
      edgeG.appendChild(tmp);
      linking = { node, port, dir, ptype: port.type, tmp };
      viewport.classList.add("linking");
      window.addEventListener("pointermove", onLinkMove);
      window.addEventListener("pointerup", onLinkUp);
    }
    function anchorForLink() {
      const a = portPoint(linking.node, opts5.manifestOf(linking.node.typeId), linking.dir, linking.port.id);
      return a;
    }
    function onLinkMove(ev) {
      if (!linking) return;
      const a = anchorForLink();
      const b = toWorld(ev.clientX, ev.clientY);
      linking.tmp.setAttribute("d", linking.dir === "out" ? bezier(a, b) : bezier(b, a));
    }
    function onLinkUp(ev) {
      window.removeEventListener("pointermove", onLinkMove);
      window.removeEventListener("pointerup", onLinkUp);
      viewport.classList.remove("linking");
      if (!linking) return;
      const src = linking;
      src.tmp.remove();
      linking = null;
      const el3 = document.elementFromPoint(ev.clientX, ev.clientY);
      const target = el3 && el3.closest ? el3.closest(".port") : null;
      if (!target) {
        const w = toWorld(ev.clientX, ev.clientY);
        opts5.onOpenSearch({ world: w, link: src });
        return;
      }
      const tNodeId = target.dataset.node;
      const tPortId = target.dataset.port;
      const tDir = target.dataset.dir;
      const tType = target.dataset.ptype;
      if (!tNodeId || tDir === src.dir) return;
      const outSide = src.dir === "out" ? src : { node: { id: tNodeId }, port: { id: tPortId, type: tType } };
      const inSide = src.dir === "in" ? src : { node: { id: tNodeId }, port: { id: tPortId, type: tType } };
      if (!portsCompatible(outSide.port.type, inSide.port.type)) {
        opts5.onStatus(`Incompatible ports: ${outSide.port.type} \u2192 ${inSide.port.type}`);
        return;
      }
      opts5.commands.replaceConnect(
        outSide.node.id,
        outSide.port.id,
        inSide.node.id,
        inSide.port.id
      );
    }
    function completePendingLink(newNodeId) {
      const pending2 = opts5.takePendingLink?.();
      if (!pending2) return;
      const t = (typeId) => opts5.manifestOf(typeId);
      if (pending2.dir === "out") {
        const newM = t(pending2.meta.typeId);
        const inPort = (newM?.inputs || []).find((p) => portsCompatible(pending2.ptype, p.type));
        if (inPort) {
          opts5.commands.replaceConnect(pending2.node.id, pending2.port.id, newNodeId, inPort.id);
        } else {
          opts5.onStatus(`No ${pending2.ptype} input on ${pending2.meta.typeId}`, true);
        }
      } else {
        const newM = t(pending2.meta.typeId);
        const outPort = (newM?.outputs || []).find((p) => portsCompatible(p.type, pending2.ptype)) || null;
        if (outPort) {
          opts5.commands.replaceConnect(newNodeId, outPort.id, pending2.node.id, pending2.port.id);
        } else {
          opts5.onStatus(`No ${pending2.ptype} output on ${pending2.meta.typeId}`, true);
        }
      }
    }
    function onViewportPointerDown(ev) {
      if (ev.target.closest && (ev.target.closest(".node") || ev.target.closest(".frame"))) return;
      if (ev.button === 1 || ev.button === 0 && ev.altKey && !ev.target.closest(".port")) {
        startPan(ev);
        return;
      }
      if (ev.button !== 0) return;
      const start = toWorld(ev.clientX, ev.clientY);
      marq = { start, add: ev.shiftKey };
      marquee.style.display = "block";
      updateMarquee(ev);
      window.addEventListener("pointermove", onMarqueeMove);
      window.addEventListener("pointerup", onMarqueeEnd);
    }
    function updateMarquee(ev) {
      const r = viewportRect();
      const a = { x: (marq.start.x - view.x) * view.zoom, y: (marq.start.y - view.y) * view.zoom };
      const b = { x: ev.clientX - r.left, y: ev.clientY - r.top };
      marquee.style.left = Math.min(a.x, b.x) + "px";
      marquee.style.top = Math.min(a.y, b.y) + "px";
      marquee.style.width = Math.abs(b.x - a.x) + "px";
      marquee.style.height = Math.abs(b.y - a.y) + "px";
    }
    function onMarqueeMove(ev) {
      if (marq) updateMarquee(ev);
    }
    function onMarqueeEnd(ev) {
      window.removeEventListener("pointermove", onMarqueeMove);
      window.removeEventListener("pointerup", onMarqueeEnd);
      marquee.style.display = "none";
      if (!marq) return;
      const end = toWorld(ev.clientX, ev.clientY);
      const r = {
        x: Math.min(marq.start.x, end.x),
        y: Math.min(marq.start.y, end.y),
        w: Math.abs(end.x - marq.start.x),
        h: Math.abs(end.y - marq.start.y)
      };
      const g = graph();
      if (g && (r.w > 4 || r.h > 4)) {
        if (!marq.add) store.state.selection.clear();
        for (const n of g.nodes) {
          const b = nodeRect(n, opts5.manifestOf(n.typeId));
          if (b.x < r.x + r.w && b.x + b.w > r.x && b.y < r.y + r.h && b.y + b.h > r.y) {
            store.state.selection.add(n.id);
          }
        }
        opts5.onSelectionChanged();
      } else if (!marq.add) {
        store.state.selection.clear();
        opts5.onSelectionChanged();
      }
      marq = null;
    }
    function startPan(ev) {
      ev.preventDefault();
      pan = { sx: ev.clientX, sy: ev.clientY, vx: view.x, vy: view.y };
      viewport.classList.add("panning");
      window.addEventListener("pointermove", onPanMove);
      window.addEventListener("pointerup", onPanEnd);
    }
    function onPanMove(ev) {
      if (!pan) return;
      setView({
        x: pan.vx - (ev.clientX - pan.sx) / view.zoom,
        y: pan.vy - (ev.clientY - pan.sy) / view.zoom,
        zoom: view.zoom
      });
    }
    function onPanEnd() {
      window.removeEventListener("pointermove", onPanMove);
      window.removeEventListener("pointerup", onPanEnd);
      viewport.classList.remove("panning");
      pan = null;
      opts5.onViewChanged?.();
    }
    function onWheel(ev) {
      ev.preventDefault();
      const r = viewportRect();
      const mx = ev.clientX - r.left;
      const my = ev.clientY - r.top;
      const wx = view.x + mx / view.zoom;
      const wy = view.y + my / view.zoom;
      const factor = Math.exp(-ev.deltaY * 15e-4);
      const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, view.zoom * factor));
      setView({ x: wx - mx / zoom, y: wy - my / zoom, zoom });
      opts5.onViewChanged?.();
    }
    function bounds() {
      const g = graph();
      if (!g || !g.nodes.length) return null;
      let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      for (const n of g.nodes) {
        const rect = nodeRect(n, opts5.manifestOf(n.typeId));
        minX = Math.min(minX, rect.x);
        minY = Math.min(minY, rect.y);
        maxX = Math.max(maxX, rect.x + rect.w);
        maxY = Math.max(maxY, rect.y + rect.h);
      }
      return { minX, minY, maxX, maxY };
    }
    function fit(padding = 70) {
      const b = bounds();
      const r = viewportRect();
      if (!b) {
        setView({ x: 0, y: 0, zoom: 1 });
        return;
      }
      const w = b.maxX - b.minX + padding * 2;
      const h = b.maxY - b.minY + padding * 2;
      const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.min(r.width / w, r.height / h)));
      setView({ x: b.minX - padding, y: b.minY - padding, zoom });
    }
    function focusNode(nodeId, padding = 120) {
      const g = graph();
      const n = g?.nodes.find((x) => x.id === nodeId);
      if (!n) return;
      const r = viewportRect();
      const rect = nodeRect(n, opts5.manifestOf(n.typeId));
      const zoom = Math.max(view.zoom, 0.9);
      setView({
        x: rect.x + rect.w / 2 - r.width / (2 * zoom),
        y: rect.y + rect.h / 2 - r.height / (2 * zoom),
        zoom
      });
    }
    function centerOn(worldPt) {
      const r = viewportRect();
      setView({ x: worldPt.x - r.width / (2 * view.zoom), y: worldPt.y - r.height / (2 * view.zoom), zoom: view.zoom });
    }
    function drawMinimap() {
      if (!minimap || minimap.classList.contains("hidden")) return;
      const b = bounds();
      const ctx = minimap.getContext("2d");
      const W = minimap.width, H = minimap.height;
      ctx.clearRect(0, 0, W, H);
      if (!b) return;
      const pad = 8;
      const sw = (W - pad * 2) / Math.max(1, b.maxX - b.minX);
      const sh = (H - pad * 2) / Math.max(1, b.maxY - b.minY);
      const s = Math.min(sw, sh);
      const ox = pad - b.minX * s;
      const oy = pad - b.minY * s;
      const g = graph();
      for (const n of g.nodes) {
        const rect = nodeRect(n, opts5.manifestOf(n.typeId));
        ctx.fillStyle = store.state.selection.has(n.id) ? "#4a9eff" : "#3a4656";
        ctx.fillRect(ox + rect.x * s, oy + rect.y * s, Math.max(2, rect.w * s), Math.max(2, rect.h * s));
      }
      const vr = viewportRect();
      ctx.strokeStyle = "#8fc4ff";
      ctx.lineWidth = 1;
      ctx.strokeRect(ox + view.x * s, oy + view.y * s, vr.width / view.zoom * s, vr.height / view.zoom * s);
    }
    function minimapClick(ev) {
      const b = bounds();
      if (!b) return;
      const rect = minimap.getBoundingClientRect();
      const pad = 8;
      const W = minimap.width, H = minimap.height;
      const sw = (W - pad * 2) / Math.max(1, b.maxX - b.minX);
      const sh = (H - pad * 2) / Math.max(1, b.maxY - b.minY);
      const s = Math.min(sw, sh);
      const ox = pad - b.minX * s;
      const oy = pad - b.minY * s;
      const wx = (ev.clientX - rect.left - ox) / s;
      const wy = (ev.clientY - rect.top - oy) / s;
      centerOn({ x: wx, y: wy });
    }
    viewport.addEventListener("pointerdown", onViewportPointerDown);
    viewport.addEventListener("wheel", onWheel, { passive: false });
    minimap.addEventListener("pointerdown", minimapClick);
    applyTransform();
    return {
      render: render4,
      renderEdges,
      updatePositions,
      fit,
      focusNode,
      centerOn,
      completePendingLink,
      selectAllConnected,
      toWorld,
      bounds,
      getView: () => view,
      setView,
      drawMinimap
    };
  }

  // view/library.js
  var filter = "";
  var host;
  var opts;
  var PRIMARY_CATS = [
    "Generators",
    "Vector",
    "Typography",
    "Image animation",
    "3D SDF",
    "3D Mesh",
    "Particles",
    "Physics",
    "Simulation",
    "Control",
    "Blur",
    "Color",
    "Blend",
    "Distortion",
    "Light",
    "Optical",
    "Material",
    "Glitch",
    "Analog",
    "Temporal",
    "Matte",
    "Style",
    "Texture",
    "Technical",
    "Print and Pixel",
    "Utility",
    "Input/Output",
    "Mask",
    "Conditional"
  ];
  function orderedCategories(cats) {
    const present = new Set(cats);
    const out = PRIMARY_CATS.filter((c) => present.has(c));
    for (const c of cats) if (!out.includes(c)) out.push(c);
    return out;
  }
  function matches(e) {
    if (!filter) return true;
    const f = filter.toLowerCase();
    return e.name.toLowerCase().includes(f) || e.id.toLowerCase().includes(f) || (e.help || "").toLowerCase().includes(f) || (e.category || "").toLowerCase().includes(f);
  }
  function loadCollapsed() {
    try {
      const raw = localStorage.getItem("flux.libCollapsed");
      if (raw) state.ui.collapsed = JSON.parse(raw);
    } catch (e) {
    }
  }
  function saveCollapsed() {
    try {
      localStorage.setItem("flux.libCollapsed", JSON.stringify(state.ui.collapsed));
    } catch (e) {
    }
  }
  function renderNodesTab() {
    var _a;
    const groups = {};
    for (const e of state.catalog) {
      if (!matches(e)) continue;
      (groups[_a = e.category] || (groups[_a] = [])).push(e);
    }
    host.innerHTML = "";
    const cats = orderedCategories(Object.keys(groups));
    if (!cats.length) {
      host.innerHTML = `<div class="lib-empty">No nodes match \u201C${filter}\u201D.</div>`;
      return;
    }
    for (const cat of cats) {
      const items = groups[cat];
      const collapsed = !!state.ui.collapsed[cat];
      const h = document.createElement("div");
      h.className = "lib-cat";
      const sw = document.createElement("span");
      sw.className = "swatch";
      sw.style.background = categoryColor(cat);
      const nm = document.createElement("span");
      nm.textContent = `${collapsed ? "\u25B8" : "\u25BE"} ${cat}`;
      const cnt = document.createElement("span");
      cnt.className = "count";
      cnt.textContent = items.length;
      h.append(sw, nm, cnt);
      h.onclick = () => {
        state.ui.collapsed[cat] = !collapsed;
        saveCollapsed();
        renderNodesTab();
      };
      host.appendChild(h);
      if (collapsed) continue;
      for (const e of items) {
        const d = document.createElement("div");
        d.className = "lib-item";
        d.draggable = true;
        d.title = e.help || e.id;
        const nmRow = document.createElement("div");
        nmRow.className = "nm";
        const name = document.createElement("span");
        name.textContent = e.name;
        const badge = document.createElement("span");
        badge.className = "badge";
        badge.textContent = e.kind;
        nmRow.append(name, badge);
        const desc = document.createElement("div");
        desc.className = "desc";
        desc.textContent = e.help || e.id;
        d.append(nmRow, desc);
        d.addEventListener("click", () => opts.onAdd(e.id));
        d.addEventListener("dragstart", (ev) => {
          ev.dataTransfer.setData("application/x-flux-node", e.id);
          ev.dataTransfer.effectAllowed = "copy";
        });
        host.appendChild(d);
      }
    }
  }
  function renderPresetsTab() {
    host.innerHTML = "";
    if (!state.presets.length) {
      host.innerHTML = `<div class="lib-empty">No presets loaded.</div>`;
      return;
    }
    for (const p of state.presets) {
      const d = document.createElement("div");
      d.className = "preset-item";
      d.draggable = true;
      const nm = document.createElement("div");
      nm.textContent = p.name;
      const desc = document.createElement("div");
      desc.className = "desc";
      desc.textContent = p.description || p.id;
      d.append(nm, desc);
      d.addEventListener("click", () => opts.onInsertPreset(p.id, null));
      d.addEventListener("dragstart", (ev) => {
        ev.dataTransfer.setData("application/x-flux-preset", p.id);
        ev.dataTransfer.effectAllowed = "copy";
      });
      host.appendChild(d);
    }
  }
  function initLibrary(options) {
    opts = options;
    host = document.getElementById("library");
    loadCollapsed();
    const search = document.getElementById("search") || host?.closest("#lib")?.querySelector('input[type="text"]');
    if (search) {
      search.addEventListener("input", () => {
        filter = search.value;
        render();
      });
    }
    document.querySelectorAll("#lib .tabs button").forEach((btn) => {
      btn.addEventListener("click", () => {
        state.ui.libTab = btn.dataset.tab;
        document.querySelectorAll("#lib .tabs button").forEach((b) => b.classList.toggle("active", b === btn));
        render();
      });
    });
    render();
    return { render };
  }
  function render() {
    if (!host) return;
    if (state.ui.libTab === "presets") renderPresetsTab();
    else renderNodesTab();
  }
  function searchNodes(q, limit = 24) {
    const f = q.toLowerCase().trim();
    const list = state.catalog.filter((e) => {
      if (!f) return true;
      return e.name.toLowerCase().includes(f) || e.id.toLowerCase().includes(f) || (e.help || "").toLowerCase().includes(f);
    });
    return list.slice(0, limit);
  }

  // view/inspector.js
  var opts2;
  function initInspector(options) {
    opts2 = options;
    render2();
    return { render: render2 };
  }
  function commitParam(node, decl, kind, value) {
    setParam(node.id, decl.id, kind, value);
    opts2?.onParamChanged?.();
  }
  function timeWire(seconds) {
    const micros = Math.round(seconds * 1e6);
    return { numerator: String(micros), denominator: 1e6 };
  }
  function currentNode() {
    const g = state.graph;
    if (!g) return null;
    const ids = [...state.selection];
    if (ids.length !== 1) return null;
    return g.nodes.find((n) => n.id === ids[0]) || null;
  }
  function valueOf(node, decl) {
    const ov = node.params?.[decl.id];
    if (ov !== void 0) return ov.value;
    return decl.default;
  }
  function laneFor(nodeId, paramId) {
    return `${nodeId}:${paramId}`;
  }
  function keyframesFor(nodeId, paramId) {
    return state.graph?.keyframes?.[laneFor(nodeId, paramId)] || [];
  }
  function nearKeyframe(list, seconds) {
    const t = seconds;
    const tol = 0.5 / (state.project?.settings?.frameRate?.num || 60);
    return list.findIndex((k) => Math.abs(Number(k.time.numerator) / k.time.denominator - t) <= tol);
  }
  function hasMapping(nodeId, paramId) {
    return Object.values(state.project?.mappings || {}).find(
      (m) => m.nodeId === nodeId && m.paramId === paramId
    );
  }
  function labelFor(decl) {
    return decl.label || decl.id;
  }
  function rangeBounds(decl, value) {
    const current = Number(value) || 0;
    let min = Number(decl.min);
    let max = Number(decl.max);
    if (!Number.isFinite(min)) min = Math.min(0, current);
    if (!Number.isFinite(max)) max = Math.max(1, current, min + 1);
    if (max <= min) max = min + 1;
    return { min, max };
  }
  function rangeControl(decl, value, onInput) {
    const input = document.createElement("input");
    input.type = "range";
    const { min, max } = rangeBounds(decl, value);
    input.min = min;
    input.max = max;
    input.step = decl.kind === "int" ? "1" : "0.001";
    input.value = Number(value) || 0;
    const readout = document.createElement("output");
    readout.className = "range-readout";
    const format = (v) => decl.kind === "int" ? String(Math.round(Number(v))) : Number(v).toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
    const update = () => {
      readout.value = format(input.value);
      readout.textContent = format(input.value);
    };
    input.addEventListener("input", () => {
      update();
      onInput(Number(input.value));
    });
    update();
    return { input, readout };
  }
  function numericRow(node, decl) {
    const row = document.createElement("div");
    row.className = "inspector-param";
    const lab = document.createElement("label");
    lab.textContent = labelFor(decl);
    lab.title = `${decl.id}${decl.unit ? " (" + decl.unit + ")" : ""}`;
    row.appendChild(lab);
    const wrap = document.createElement("div");
    wrap.className = "val";
    const control = rangeControl(decl, valueOf(node, decl), (v) => {
      commitParam(node, decl, decl.kind, v);
    });
    wrap.append(control.input, control.readout);
    row.appendChild(wrap);
    attachAnimButtons(row, node, decl);
    return row;
  }
  function boolRow(node, decl) {
    const row = document.createElement("div");
    row.className = "inspector-param";
    const lab = document.createElement("label");
    lab.textContent = labelFor(decl);
    row.appendChild(lab);
    const wrap = document.createElement("div");
    wrap.className = "val";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = !!valueOf(node, decl);
    input.addEventListener("change", () => commitParam(node, decl, "bool", input.checked));
    wrap.appendChild(input);
    row.appendChild(wrap);
    return row;
  }
  function enumRow(node, decl) {
    const row = document.createElement("div");
    row.className = "inspector-param";
    const lab = document.createElement("label");
    lab.textContent = labelFor(decl);
    row.appendChild(lab);
    const wrap = document.createElement("div");
    wrap.className = "val";
    const sel = document.createElement("select");
    (decl.options || []).forEach((opt, i) => {
      const o = document.createElement("option");
      o.value = i;
      o.textContent = opt;
      sel.appendChild(o);
    });
    const current = valueOf(node, decl);
    sel.value = typeof current === "number" ? current : 0;
    sel.addEventListener("change", () => commitParam(node, decl, "enum", Number(sel.value)));
    wrap.appendChild(sel);
    row.appendChild(wrap);
    return row;
  }
  function textRow(node, decl) {
    const row = document.createElement("div");
    row.className = "inspector-param inspector-text";
    const lab = document.createElement("label");
    lab.textContent = labelFor(decl);
    const input = document.createElement("textarea");
    input.rows = 3;
    input.value = valueOf(node, decl) ?? "";
    input.placeholder = "Enter text\u2026";
    input.addEventListener("change", () => commitParam(node, decl, "text", input.value));
    row.append(lab, input);
    return row;
  }
  function colorRow(node, decl) {
    const row = document.createElement("div");
    row.className = "inspector-param";
    const lab = document.createElement("label");
    lab.textContent = labelFor(decl);
    row.appendChild(lab);
    const wrap = document.createElement("div");
    wrap.className = "val";
    const current = valueOf(node, decl);
    const arr = Array.isArray(current) ? current.slice() : [1, 1, 1, 1];
    for (let i = 0; i < 4; i++) {
      const c = document.createElement("input");
      c.type = "range";
      c.step = "0.01";
      c.min = "0";
      c.max = "1";
      c.value = arr[i] ?? (i === 3 ? 1 : 0);
      const value = document.createElement("output");
      value.className = "range-readout";
      value.textContent = Number(c.value).toFixed(2);
      c.title = ["R", "G", "B", "A"][i];
      c.addEventListener("input", () => {
        const v = arr.slice();
        v[i] = Number(c.value);
        value.textContent = Number(c.value).toFixed(2);
        setParam(node.id, decl.id, "color", v);
        opts2?.onParamChanged?.();
      });
      wrap.append(c, value);
    }
    row.appendChild(wrap);
    return row;
  }
  function vecRow(node, decl) {
    const n = decl.kind === "vec2" ? 2 : 3;
    const row = document.createElement("div");
    row.className = "inspector-param";
    const lab = document.createElement("label");
    lab.textContent = labelFor(decl);
    row.appendChild(lab);
    const wrap = document.createElement("div");
    wrap.className = "val";
    const current = valueOf(node, decl);
    const arr = Array.isArray(current) ? current.slice() : new Array(n).fill(0);
    for (let i = 0; i < n; i++) {
      const c = document.createElement("input");
      c.type = "range";
      c.step = "0.001";
      const bounds = rangeBounds(decl, arr[i] ?? 0);
      c.min = bounds.min;
      c.max = bounds.max;
      c.value = arr[i] ?? 0;
      const value = document.createElement("output");
      value.className = "range-readout";
      value.textContent = Number(c.value).toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
      c.addEventListener("input", () => {
        const v = arr.slice();
        v[i] = Number(c.value);
        value.textContent = Number(c.value).toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
        setParam(node.id, decl.id, decl.kind, v);
        opts2?.onParamChanged?.();
      });
      wrap.append(c, value);
    }
    row.appendChild(wrap);
    return row;
  }
  function attachAnimButtons(row, node, decl) {
    const wrap = row.querySelector(".val");
    if (decl.automatable) {
      const b = document.createElement("button");
      b.className = "auto";
      b.title = "Toggle keyframe at the playhead";
      b.textContent = "\u25C6";
      const keys = keyframesFor(node.id, decl.id);
      if (nearKeyframe(keys, state.time) >= 0) b.classList.add("active");
      b.addEventListener("click", () => toggleKeyframe(node, decl));
      wrap.appendChild(b);
    }
    if (decl.modulatable) {
      const m = hasMapping(node.id, decl.id);
      const b = document.createElement("button");
      b.className = "auto" + (m ? " active" : "");
      b.title = m ? "Remove LFO modulation" : "Add LFO modulation";
      b.textContent = "\u223F";
      b.addEventListener("click", () => toggleModulation(node, decl, m));
      wrap.appendChild(b);
    }
  }
  function toggleKeyframe(node, decl) {
    const lane = laneFor(node.id, decl.id);
    const list = keyframesFor(node.id, decl.id).slice();
    const idx = nearKeyframe(list, state.time);
    if (idx >= 0) {
      list.splice(idx, 1);
    } else {
      const value = paramValue(decl.kind, valueOf(node, decl));
      list.push({ time: timeWire(state.time), value, interpolation: "linear" });
      list.sort((a, b) => Number(a.time.numerator) / a.time.denominator - Number(b.time.numerator) / b.time.denominator);
    }
    setKeyframes(lane, list).then(() => opts2?.onGraphChanged?.());
  }
  function toggleModulation(node, decl, existing) {
    if (existing) {
      removeMapping(existing.id).then(() => opts2?.onGraphChanged?.());
      return;
    }
    const id = "map-" + Math.random().toString(36).slice(2) + Date.now().toString(36);
    setMapping({
      id,
      nodeId: node.id,
      paramId: decl.id,
      source: { kind: "lfo", shape: "sine", frequency: 0.5, phase: 0 },
      inMin: 0,
      inMax: 1,
      amount: 0.5,
      fold: "add",
      order: 0,
      invert: false,
      attackMs: 0,
      releaseMs: 0,
      enabled: true
    }).then(() => opts2?.onGraphChanged?.());
  }
  function paramRow(node, decl) {
    switch (decl.kind) {
      case "bool":
        return boolRow(node, decl);
      case "enum":
        return enumRow(node, decl);
      case "text":
        return textRow(node, decl);
      case "color":
        return colorRow(node, decl);
      case "vec2":
      case "vec3":
        return vecRow(node, decl);
      default:
        return numericRow(node, decl);
    }
  }
  function el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }
  function renderEmpty() {
    const host3 = document.getElementById("inspector");
    const g = state.graph;
    host3.innerHTML = "";
    host3.appendChild(el("h2", null, g?.name || "Composition"));
    if (!g) {
      host3.appendChild(el("div", "muted", "No graph."));
      return;
    }
    host3.appendChild(el("div", "muted", `${g.nodes.length} nodes \xB7 ${g.edges.length} cables`));
    host3.appendChild(el("hr"));
    host3.appendChild(el("div", "muted", "Select a node to edit its parameters."));
    const hints = el("div", "dim");
    hints.style.marginTop = "10px";
    hints.textContent = "Space: add node \xB7 drag a port: connect \xB7 Tab: inspector \xB7 1: fit \xB7 2: fit selected";
    host3.appendChild(hints);
  }
  function renderMulti(nodes) {
    const host3 = document.getElementById("inspector");
    host3.innerHTML = "";
    host3.appendChild(el("h2", null, `${nodes.length} nodes selected`));
    host3.appendChild(el("hr"));
    const align = el("div", "btnrow");
    const mk = (label, fn) => {
      const b = el("button", null, label);
      b.addEventListener("click", fn);
      return b;
    };
    align.append(
      mk("Align left", () => alignNodes(nodes, "left")),
      mk("Align top", () => alignNodes(nodes, "top"))
    );
    host3.appendChild(align);
    const row = el("div", "btnrow");
    const del = mk("Delete", () => opts2.commands.removeNodes(nodes.map((n) => n.id)));
    del.style.color = "var(--err)";
    row.appendChild(del);
    host3.appendChild(row);
  }
  function alignNodes(nodes, mode) {
    if (mode === "left") {
      const x = Math.min(...nodes.map((n) => n.position[0]));
      moveNodes(nodes.map((n) => ({ nodeId: n.id, x, y: n.position[1] })));
    } else {
      const y = Math.min(...nodes.map((n) => n.position[1]));
      moveNodes(nodes.map((n) => ({ nodeId: n.id, x: n.position[0], y })));
    }
  }
  function randomizeValue(decl) {
    const unit = () => Math.random();
    const range = () => {
      const min = Number.isFinite(decl.min) ? decl.min : 0;
      const max = Number.isFinite(decl.max) ? decl.max : 1;
      return min + unit() * (max - min);
    };
    switch (decl.kind) {
      case "bool":
        return Math.random() >= 0.5;
      case "enum":
        return Math.floor(unit() * Math.max(1, (decl.options || []).length));
      case "text":
        return ["FLUX", "GENFX", "Hello world", "VIDEO", "0101"][Math.floor(unit() * 5)];
      case "color":
        return [unit(), unit(), unit(), 1];
      case "vec2":
        return [range(), range()];
      case "vec3":
        return [range(), range(), range()];
      case "int":
        return Math.round(range());
      default:
        return range();
    }
  }
  async function randomizeNode(node, decls) {
    await Promise.all(decls.map((d) => opts2.commands.setParam(node.id, d.id, d.kind, randomizeValue(d))));
    opts2?.onParamChanged?.();
  }
  function renderSingle(node) {
    const host3 = document.getElementById("inspector");
    const m = manifestOf(node.typeId);
    host3.innerHTML = "";
    const h = el("h2");
    const dot = el("span");
    dot.style.cssText = `display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:6px;background:${categoryColor(m?.category || "Utility")}`;
    h.append(dot, document.createTextNode(m?.name ?? node.typeId));
    host3.appendChild(h);
    host3.appendChild(el("div", "muted", `${node.typeId} \xB7 ${m?.category || "?"}${m?.kind ? " \xB7 " + m.kind : ""}`));
    if (m?.help) {
      const help = el("div", "dim");
      help.style.marginTop = "4px";
      help.textContent = m.help;
      host3.appendChild(help);
    }
    if (node.missingType) host3.appendChild(el("div", null, "\u26A0 type unavailable")).style.color = "var(--err)";
    host3.appendChild(el("hr"));
    const decls = (m?.parameters || []).filter((d) => !d.hidden);
    host3.appendChild(el("div", "sec", "Parameters"));
    if (!decls.length) {
      host3.appendChild(el("div", "muted", "No editable parameters."));
    }
    for (const d of decls) host3.appendChild(paramRow(node, d));
    host3.appendChild(el("div", "sec", "Connections"));
    const g = state.graph;
    const list = el("div", "conn-list");
    const inputs = m?.inputs || [];
    if (!inputs.length) list.appendChild(el("div", "dim", "No inputs."));
    for (const p of inputs) {
      const e = g.edges.find((x) => x.toNode === node.id && x.toPort === p.id);
      const row3 = el("div", "ci");
      const pp = el("span", "pp", p.id);
      const txt = el("span", null, e ? `\u2190 ${nameOf(e.fromNode)}:${e.fromPort}` : `\u2014 not connected${p.required ? " (required)" : ""}`);
      row3.append(pp, txt);
      if (e) row3.addEventListener("click", () => opts2.onFocusNode(e.fromNode));
      list.appendChild(row3);
    }
    host3.appendChild(list);
    host3.appendChild(el("div", "sec", "Actions"));
    const row = el("div", "btnrow");
    const byp = el("button", node.bypass ? "on" : null, node.bypass ? "Bypassed" : "Bypass");
    byp.addEventListener("click", () => {
      setBypass(node.id, !node.bypass);
      opts2?.onGraphChanged?.();
    });
    row.appendChild(byp);
    const reset = el("button", null, "Reset");
    reset.addEventListener("click", () => {
      for (const d of decls) setParam(node.id, d.id, d.kind, d.default);
      opts2?.onParamChanged?.();
    });
    row.appendChild(reset);
    if (m?.kind === "generator" || m?.generator === true || node.typeId.startsWith("gen.")) {
      const random = el("button", null, "Randomize");
      random.title = "Randomize every editable generator setting";
      random.addEventListener("click", () => randomizeNode(node, decls));
      row.appendChild(random);
    }
    if (node.typeId !== "flux.output") {
      const setOut = el("button", null, "Set output");
      setOut.title = "Render this node as the composition result";
      setOut.addEventListener("click", () => {
        setGraphOutput(node.id);
        opts2?.onGraphChanged?.();
      });
      row.appendChild(setOut);
    }
    host3.appendChild(row);
    const row2 = el("div", "btnrow");
    const del = el("button", null, "Delete node");
    del.style.color = "var(--err)";
    del.addEventListener("click", () => {
      state.selection.clear();
      removeNode(node.id);
    });
    row2.appendChild(del);
    host3.appendChild(row2);
  }
  function nameOf(nodeId) {
    const n = state.graph?.nodes.find((x) => x.id === nodeId);
    if (!n) return nodeId;
    const m = manifestOf(n.typeId);
    return m?.name ?? n.typeId;
  }
  function render2() {
    const host3 = document.getElementById("inspector");
    if (!host3) return;
    const ids = [...state.selection];
    if (ids.length === 0) return renderEmpty();
    if (ids.length > 1) {
      const nodes = state.graph ? state.graph.nodes.filter((n) => state.selection.has(n.id)) : [];
      return renderMulti(nodes);
    }
    const node = currentNode();
    if (!node) return renderEmpty();
    renderSingle(node);
  }

  // view/timeline.js
  var opts3;
  var host2;
  var head;
  function durationSeconds() {
    const d = state.project?.timeline?.range?.duration;
    if (!d) return 8;
    const s = Number(d.numerator) / d.denominator;
    return s > 0 ? s : 8;
  }
  function laneLabel(lane) {
    const [nodeId, param] = lane.split(":");
    const n = state.graph?.nodes.find((x) => x.id === nodeId);
    const m = n ? manifestOf(n.typeId) : null;
    const label = (m?.parameters || []).find((p) => p.id === param)?.label || param;
    return `${m?.name ?? nodeId} \xB7 ${label}`;
  }
  function initTimeline(options) {
    opts3 = options;
    host2 = document.getElementById("timeline");
    head = document.getElementById("timelineHead");
    render3();
    return { render: render3 };
  }
  function render3() {
    if (!host2) return;
    host2.innerHTML = "";
    const g = state.graph;
    if (!g) {
      host2.appendChild(el2("div", "dim", "No composition."));
      return;
    }
    const dur = durationSeconds();
    const lanes = Object.entries(g.keyframes || {}).filter(([, keys]) => keys.length);
    const bar = el2("div", "tl-bar");
    const playhead = el2("div", "tl-play");
    playhead.style.left = `${Math.min(state.time, dur) / dur * 100}%`;
    bar.appendChild(playhead);
    bar.addEventListener("pointerdown", (ev) => {
      const r = bar.getBoundingClientRect();
      const t = Math.max(0, Math.min(dur, (ev.clientX - r.left) / r.width * dur));
      opts3.onSeek(t);
    });
    host2.appendChild(bar);
    if (!lanes.length) {
      host2.appendChild(el2("div", "dim", "No keyframes. Use \u25C6 on a parameter to animate it."));
      return;
    }
    for (const [lane, keys] of lanes) {
      const row = el2("div", "tl-lane");
      const name = el2("span", "tl-name", laneLabel(lane));
      row.appendChild(name);
      const track = el2("div", "tl-track");
      for (const k of keys) {
        const t = Number(k.time.numerator) / k.time.denominator;
        const d = el2("div", "tl-key");
        d.style.left = `${Math.min(100, t / dur * 100)}%`;
        d.title = `${t.toFixed(3)}s`;
        d.addEventListener("pointerdown", (ev) => {
          ev.stopPropagation();
          opts3.onSeek(t);
        });
        d.addEventListener("dblclick", (ev) => {
          ev.stopPropagation();
          opts3.onRemoveKeyframe(lane, k);
        });
        track.appendChild(d);
      }
      row.appendChild(track);
      host2.appendChild(row);
    }
  }
  function el2(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }

  // view/overlay.js
  var opts4;
  var results = [];
  var active = 0;
  var pendingContext = null;
  function initOverlay(options) {
    opts4 = options;
    const overlay2 = document.getElementById("searchOverlay");
    const input = document.getElementById("searchInput");
    const list = document.getElementById("searchResults");
    function draw() {
      results = searchNodes(input.value, 30);
      active = Math.min(active, Math.max(0, results.length - 1));
      list.innerHTML = "";
      if (!results.length) {
        const d = document.createElement("div");
        d.className = "sr dim";
        d.textContent = "No matches";
        list.appendChild(d);
        return;
      }
      results.forEach((e, i) => {
        const d = document.createElement("div");
        d.className = "sr" + (i === active ? " active" : "");
        const nm = document.createElement("span");
        nm.textContent = e.name;
        const id = document.createElement("span");
        id.className = "dim";
        id.textContent = e.id;
        const cat = document.createElement("span");
        cat.className = "cat";
        cat.textContent = e.category;
        d.append(nm, id, cat);
        d.addEventListener("pointerdown", (ev) => {
          ev.preventDefault();
          choose(i);
        });
        list.appendChild(d);
      });
    }
    function choose(i) {
      const e = results[i];
      if (!e) return;
      close();
      opts4.onPick(e.id, pendingContext);
      pendingContext = null;
    }
    function open(ctx) {
      pendingContext = ctx || null;
      active = 0;
      input.value = "";
      overlay2.classList.add("open");
      draw();
      input.focus();
    }
    function close() {
      overlay2.classList.remove("open");
    }
    input.addEventListener("input", () => {
      active = 0;
      draw();
    });
    input.addEventListener("keydown", (ev) => {
      if (ev.key === "ArrowDown") {
        ev.preventDefault();
        active = Math.min(results.length - 1, active + 1);
        draw();
      } else if (ev.key === "ArrowUp") {
        ev.preventDefault();
        active = Math.max(0, active - 1);
        draw();
      } else if (ev.key === "Enter") {
        ev.preventDefault();
        choose(active);
      } else if (ev.key === "Escape") {
        ev.preventDefault();
        close();
      }
    });
    overlay2.addEventListener("pointerdown", (ev) => {
      if (ev.target === overlay2) close();
    });
    const menu = document.getElementById("contextMenu");
    function openMenu(clientX, clientY, items) {
      menu.innerHTML = "";
      for (const it of items) {
        const d = document.createElement("div");
        d.className = "mi" + (it.danger ? " danger" : "");
        d.textContent = it.label;
        d.addEventListener("click", () => {
          closeMenu();
          it.fn();
        });
        menu.appendChild(d);
      }
      menu.style.left = clientX + "px";
      menu.style.top = clientY + "px";
      menu.classList.add("open");
    }
    function closeMenu() {
      menu.classList.remove("open");
    }
    window.addEventListener("pointerdown", (ev) => {
      if (!menu.contains(ev.target)) closeMenu();
    }, true);
    window.addEventListener("blur", closeMenu);
    const monitor = document.getElementById("monitor");
    const mImg = document.getElementById("monitorImg");
    const mHead = document.getElementById("monitorHead");
    let mDrag = null;
    mHead.addEventListener("pointerdown", (ev) => {
      if (ev.target.tagName === "BUTTON") return;
      const r = monitor.getBoundingClientRect();
      mDrag = { dx: ev.clientX - r.left, dy: ev.clientY - r.top };
      window.addEventListener("pointermove", onMDrag);
      window.addEventListener("pointerup", onMUp);
    });
    function onMDrag(ev) {
      if (!mDrag) return;
      monitor.style.left = ev.clientX - mDrag.dx + "px";
      monitor.style.top = ev.clientY - mDrag.dy + "px";
      monitor.style.right = "auto";
      monitor.style.bottom = "auto";
    }
    function onMUp() {
      window.removeEventListener("pointermove", onMDrag);
      window.removeEventListener("pointerup", onMUp);
      mDrag = null;
    }
    return {
      open,
      close,
      openMenu,
      closeMenu,
      toggleMonitor(force) {
        const on = force != null ? force : !monitor.classList.contains("open");
        monitor.classList.toggle("open", on);
        return on;
      },
      setMonitorImage(src) {
        mImg.src = src;
      },
      /** Paint raw RGBA into the monitor canvas (fast path). */
      setMonitorFrame(w, h, u8) {
        const cv = document.getElementById("monitorCanvas");
        if (!cv) return;
        if (cv.width !== w || cv.height !== h) {
          cv.width = w;
          cv.height = h;
        }
        const ctx = cv.getContext("2d", { alpha: true });
        const view = u8 instanceof Uint8Array ? new Uint8ClampedArray(u8.buffer, u8.byteOffset, u8.byteLength) : new Uint8ClampedArray(u8);
        ctx.putImageData(new ImageData(view, w, h), 0, 0);
        mImg.style.display = "none";
        cv.style.display = "block";
      },
      isMonitorOpen: () => monitor.classList.contains("open")
    };
  }

  // main.js
  var canvas;
  var library;
  var inspector;
  var timeline;
  var overlay;
  window.addEventListener("error", (e) => {
    const el3 = document.querySelector("#status .msg");
    if (el3) {
      el3.textContent = "UI error: " + (e.message || e.error);
      el3.classList.add("err");
    }
  });
  window.addEventListener("unhandledrejection", (e) => {
    const el3 = document.querySelector("#status .msg");
    if (el3) {
      el3.textContent = "UI error: " + (e.reason?.message || e.reason);
      el3.classList.add("err");
    }
  });
  var statusMsgEl = () => document.querySelector("#status .msg");
  var statusStatEl = () => document.querySelector("#status .stat");
  function setStatus(msg, isErr = false) {
    const el3 = statusMsgEl();
    if (!el3) return;
    el3.textContent = msg;
    el3.classList.toggle("err", !!isErr);
  }
  function updateStats() {
    const el3 = statusStatEl();
    if (!el3) return;
    const g = state.graph;
    const z = Math.round((state.view?.zoom || 1) * 100);
    el3.textContent = `rev ${state.revision} \xB7 ${g ? g.nodes.length + "n " + g.edges.length + "e" : "no graph"} \xB7 ${z}%`;
    const badge = document.getElementById("revBadge");
    if (badge) badge.textContent = `rev ${state.revision}`;
    const name = document.getElementById("projName");
    if (name && state.project?.name) name.textContent = state.project.name;
  }
  function updatePerf() {
    const el3 = document.getElementById("perfStat");
    if (!el3) return;
    el3.textContent = `preview ${perf.fps || 0}fps \xB7 ${perf.lastMs}ms`;
  }
  function reaches(from, target) {
    const g = state.graph;
    if (!g) return false;
    const seen = /* @__PURE__ */ new Set();
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
  async function replaceConnect(fromNode, fromPort, toNode, toPort) {
    if (fromNode !== toNode && reaches(toNode, fromNode) && !(isTemporalNode(fromNode) || isTemporalNode(toNode))) {
      setStatus("Connection refused: would create a cycle", true);
      return;
    }
    const existing = (state.graph?.edges || []).find((e) => e.toNode === toNode && e.toPort === toPort);
    if (existing && !(existing.fromNode === fromNode && existing.fromPort === fromPort)) {
      await disconnect(toNode, toPort);
    }
    await connect(fromNode, fromPort, toNode, toPort);
  }
  var renderScheduled = false;
  function scheduleRender() {
    if (renderScheduled) return;
    renderScheduled = true;
    requestAnimationFrame(() => {
      renderScheduled = false;
      canvas?.render();
      render2();
      render3();
      updateStats();
    });
  }
  function renderAll() {
    canvas?.render();
    render2();
    render3();
    updateStats();
  }
  var thumbCache = /* @__PURE__ */ new Map();
  var previewBusy = false;
  var previewPending = false;
  var previewTimer = null;
  function requestPreviewSoon() {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(() => requestPreviews(), 40);
  }
  function setThumb(nodeId, w, h, u8) {
    thumbCache.set(nodeId, { w, h, u8 });
    paintThumb(document.querySelector(`canvas[data-thumb-for="${nodeId}"]`), nodeId);
  }
  function paintThumb(canvas2, nodeId) {
    if (!canvas2) return;
    const rec = thumbCache.get(nodeId);
    if (!rec) return;
    drawRgba(canvas2, rec.w, rec.h, rec.u8);
  }
  function fillThumb(canvas2, nodeId) {
    paintThumb(canvas2, nodeId);
  }
  function drawRgba(canvas2, w, h, u8) {
    if (!canvas2 || !w || !h || !u8) return;
    if (canvas2.width !== w || canvas2.height !== h) {
      canvas2.width = w;
      canvas2.height = h;
    }
    const ctx = canvas2.getContext("2d", { alpha: true });
    const view = u8 instanceof Uint8Array ? new Uint8ClampedArray(u8.buffer, u8.byteOffset, u8.byteLength) : new Uint8ClampedArray(u8);
    ctx.putImageData(new ImageData(view, w, h), 0, 0);
  }
  var perf = { frames: 0, lastFpsAt: 0, fps: 0, lastMs: 0 };
  function recordPerf(ms) {
    perf.lastMs = Math.round(ms);
    perf.frames++;
    const now = performance.now();
    if (!perf.lastFpsAt) perf.lastFpsAt = now;
    if (now - perf.lastFpsAt >= 1e3) {
      perf.fps = Math.round(perf.frames * 1e3 / (now - perf.lastFpsAt));
      perf.frames = 0;
      perf.lastFpsAt = now;
    }
  }
  function thumbSize() {
    const dpr = Math.min(2, globalThis.devicePixelRatio || 1);
    const cssW = 176;
    const w = Math.min(960, Math.max(160, Math.round(cssW * dpr)));
    return { w, h: Math.round(w * 9 / 16) };
  }
  function monitorSize() {
    return { w: 960, h: 540 };
  }
  function requestPreviews() {
    if (state.exporting) return;
    if (previewBusy) {
      previewPending = true;
      return;
    }
    previewBusy = true;
    (async () => {
      try {
        const t0 = performance.now();
        const g = state.graph;
        const outputs = (g?.nodes || []).filter((n) => n.typeId === "flux.output");
        const { w, h } = thumbSize();
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
        if (previewPending) {
          previewPending = false;
          requestPreviews();
        }
      }
    })();
  }
  function centerWorld() {
    const vp = document.getElementById("viewport");
    const r = vp.getBoundingClientRect();
    return canvas.toWorld(r.left + r.width / 2, r.top + r.height / 2);
  }
  function newId() {
    try {
      return crypto.randomUUID();
    } catch (e) {
      return "n-" + Math.random().toString(36).slice(2) + Date.now().toString(36);
    }
  }
  async function addAt(typeId, pt, skipAutoWire = false) {
    try {
      const res = await addNode(typeId, pt.x, pt.y);
      const id = res?.result?.nodeId;
      if (!id) {
        setStatus(`Could not add ${typeId}: the engine returned no node id`, true);
        return null;
      }
      state.selection = /* @__PURE__ */ new Set([id]);
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
      const res = await insertPreset(presetId, pt, mergeInto);
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
  async function autoWire(newId2, typeId) {
    const g = state.graph;
    const m = manifestOf(typeId);
    if (!g || !m) return;
    const out = g.nodes.find((n) => n.typeId === "flux.output");
    if (!out) return;
    const outType = (manifestOf("flux.output")?.inputs || []).find((p) => p.id === "source")?.type || "texture2d";
    const outEdge = g.edges.find((e) => e.toNode === out.id && e.toPort === "source");
    const provider = outEdge ? outEdge.fromNode : null;
    const outPortOfType = (type, want) => (manifestOf(type)?.outputs || []).find((p) => portsCompatible(p.type, want))?.id || null;
    if (m.kind === "generator") {
      const op = outPortOfType(typeId, outType);
      if (!op) {
        setStatus(`${typeId} has no ${outType} output`, true);
        return;
      }
      if (outEdge) await disconnect(out.id, "source");
      await connect(newId2, op, out.id, "source");
      return;
    }
    const srcIn = (m.inputs || []).find((p) => p.id === "source");
    if (!srcIn || !provider || provider === newId2) return;
    const provNode = g.nodes.find((n) => n.id === provider);
    const provOut = provNode ? outPortOfType(provNode.typeId, srcIn.type) : null;
    const newOut = outPortOfType(typeId, outType);
    if (!provOut || !newOut) {
      setStatus(`Cannot auto-wire ${typeId}: no matching ports`, true);
      return;
    }
    if (outEdge) await disconnect(out.id, "source");
    await connect(provider, provOut, newId2, srcIn.id);
    await connect(newId2, newOut, out.id, "source");
  }
  async function duplicateSelection(ids) {
    const g = state.graph;
    if (!g || !ids.length) return;
    const map = /* @__PURE__ */ new Map();
    const created = [];
    for (const id of ids) {
      const n = g.nodes.find((x) => x.id === id);
      if (!n) continue;
      const nid = newId();
      map.set(id, nid);
      const copy = {
        ...JSON.parse(JSON.stringify(n)),
        id: nid,
        position: [n.position[0] + 28, n.position[1] + 28]
      };
      created.push(copy);
      await addNodeObj(copy);
    }
    for (const e of g.edges) {
      if (map.has(e.fromNode) && map.has(e.toNode)) {
        await connect(map.get(e.fromNode), e.fromPort, map.get(e.toNode), e.toPort);
      }
    }
    state.selection = new Set(created.map((c) => c.id));
    renderAll();
    requestPreviews();
  }
  async function insertReroute(edge) {
    const g = state.graph;
    const from = g?.nodes.find((n) => n.id === edge.fromNode);
    const to = g?.nodes.find((n) => n.id === edge.toNode);
    if (!from || !to) return;
    const mx = (from.position[0] + to.position[0]) / 2 + 88;
    const my = (from.position[1] + to.position[1]) / 2;
    const id = await addAt("flux.reroute", { x: mx, y: my });
    if (!id) return;
    await disconnect(edge.toNode, edge.toPort);
    await connect(edge.fromNode, edge.fromPort, id, "source");
    await connect(id, "color", edge.toNode, edge.toPort);
    state.selection = /* @__PURE__ */ new Set([id]);
    renderAll();
  }
  function frameSelection() {
    const g = state.graph;
    const nodes = selectedNodes();
    if (!g || !nodes.length) return;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const n of nodes) {
      const r = nodeRect(n, manifestOf(n.typeId));
      minX = Math.min(minX, r.x);
      minY = Math.min(minY, r.y);
      maxX = Math.max(maxX, r.x + r.w);
      maxY = Math.max(maxY, r.y + r.h);
    }
    const pad = 22;
    state.frames.push({
      id: newId(),
      title: "Frame",
      rect: [minX - pad, minY - pad, maxX - minX + pad * 2, maxY - minY + pad * 2]
    });
    persistCanvasMeta();
    canvas.render();
  }
  var metaTimer = null;
  function persistCanvasMeta() {
    clearTimeout(metaTimer);
    metaTimer = setTimeout(() => {
      setCanvasMeta(state.frames, state.view);
    }, 400);
  }
  function onContextMenu(ev) {
    ev.preventDefault();
    const nodeEl = ev.target.closest?.(".node");
    if (nodeEl) {
      const id = nodeEl.dataset.nodeId;
      if (!state.selection.has(id)) {
        state.selection = /* @__PURE__ */ new Set([id]);
        renderAll();
      }
      const node = state.graph?.nodes.find((n) => n.id === id);
      overlay.openMenu(ev.clientX, ev.clientY, [
        { label: "Duplicate", fn: () => duplicateSelection([...state.selection]) },
        { label: node?.bypass ? "Un-bypass" : "Bypass", fn: () => setBypass(id, !node?.bypass) },
        { label: "Set as output", fn: () => setGraphOutput(id) },
        { label: "Delete", danger: true, fn: () => {
          state.selection.delete(id);
          removeNodes([id]);
        } }
      ]);
      return;
    }
    overlay.openMenu(ev.clientX, ev.clientY, [
      { label: "Add node\u2026", fn: () => overlay.open({ world: canvas.toWorld(ev.clientX, ev.clientY) }) },
      { label: "Frame selection", fn: frameSelection },
      { label: "Fit graph", fn: () => canvas.fit() },
      { label: state.ui.minimap ? "Hide minimap" : "Show minimap", fn: toggleMinimap }
    ]);
  }
  function toggleMinimap() {
    state.ui.minimap = !state.ui.minimap;
    document.getElementById("minimap").classList.toggle("hidden", !state.ui.minimap);
    canvas.drawMinimap();
  }
  state.exporting = false;
  state.loopMode = (() => {
    try {
      return localStorage.getItem("flux.loopMode") || "forward";
    } catch (e) {
      return "forward";
    }
  })();
  state.playDir = 1;
  function applyLoopButton() {
    const btn = document.getElementById("loopBtn");
    if (!btn) return;
    btn.textContent = state.loopMode === "pingpong" ? "Loop: Ping" : "Loop: Fwd";
    btn.classList.toggle("on", state.loopMode === "pingpong");
  }
  async function startNewComposition() {
    await newGraph("Composition");
    state.selection.clear();
    await addAt("gen.plasma", { x: 0, y: 0 });
    renderAll();
    canvas.fit();
    requestPreviews();
  }
  function toggleLoopMode() {
    state.loopMode = state.loopMode === "pingpong" ? "forward" : "pingpong";
    state.playDir = 1;
    try {
      localStorage.setItem("flux.loopMode", state.loopMode);
    } catch (e) {
    }
    applyLoopButton();
  }
  function playbackTick(dtSeconds) {
    const dur = durationSeconds2();
    const dt = dtSeconds;
    if (state.loopMode === "pingpong") {
      state.time += dt * state.playDir;
      if (state.time >= dur) {
        state.time = Math.max(0, dur - 1e-3);
        state.playDir = -1;
      } else if (state.time <= 0) {
        state.time = 0;
        state.playDir = 1;
      }
    } else {
      state.time += dt;
      if (state.time >= dur) state.time = state.time % Math.max(1e-3, dur);
    }
    requestPreviews();
    render3();
  }
  var playRaf = 0;
  var lastTick = 0;
  function playbackLoop(now) {
    if (!state.playing) return;
    if (!lastTick) lastTick = now;
    const dt = Math.min(0.25, (now - lastTick) / 1e3);
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
  function durationSeconds2() {
    const d = state.project?.timeline?.range?.duration;
    if (!d) return 8;
    const s = Number(d.numerator) / d.denominator;
    return s > 0 ? s : 8;
  }
  function seek(t) {
    state.time = Math.max(0, Math.min(durationSeconds2(), t));
    requestPreviews();
    render3();
  }
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
      (ev.shiftKey ? redo() : undo()).then(() => {
        renderAll();
        requestPreviews();
      });
      return;
    }
    if (mod && ev.key.toLowerCase() === "y") {
      ev.preventDefault();
      redo().then(renderAll);
      return;
    }
    if (mod && ev.key.toLowerCase() === "d") {
      ev.preventDefault();
      duplicateSelection([...state.selection]);
      return;
    }
    if (mod && ev.key.toLowerCase() === "s") {
      ev.preventDefault();
      saveNow();
      return;
    }
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
        for (const n of sel) setBypass(n.id, !n.bypass);
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
          removeNodes(ids).then(() => {
            renderAll();
            requestPreviews();
          });
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
  function saveNow() {
    const path = document.getElementById("savePath").value || "project.flux.json";
    saveProject(path).then((res) => {
      if (res && !res.error) setStatus(`saved \u2192 ${path}`);
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
    const res = await loadProject(text);
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
    if (!overlay.isMonitorOpen()) {
      overlay.toggleMonitor(true);
      document.getElementById("monitorBtn").classList.add("on");
    }
    startPlayback();
    state.exporting = true;
    setStatus(`exporting ${codec.toUpperCase()}\u2026`);
    try {
      const msg = await exportProject(outDir, w, h, codec, keepFrames);
      if (msg === "export started") setStatus("exporting\u2026 rendering frames");
      else setStatus(String(msg));
    } catch (e) {
      state.exporting = false;
      requestPreviews();
      setStatus("export failed: " + (e?.message || e), true);
    }
  }
  function makeTabs() {
    document.querySelectorAll("#lib .tabs button").forEach((btn) => {
      if (btn.dataset.tab === state.ui.libTab) {
        document.querySelectorAll("#lib .tabs button").forEach((b) => b.classList.toggle("active", b === btn));
      }
    });
  }
  async function boot() {
    window.__fluxBooted = true;
    setStatusSink((m) => setStatus(m, true));
    setStatus("starting Flux\u2026");
    buildInfo().then((info) => {
      const el3 = document.getElementById("buildStat");
      if (el3 && info) el3.textContent = `build ${info.gitHash || "?"} v${info.version || ""}`;
    });
    await createProject("Untitled", 1, 1280, 720, 60, 1);
    const cat = await catalog();
    if (cat?.result?.entries) setCatalog(cat.result.entries);
    const presets = await listPresets();
    if (presets?.result?.presets) state.presets = presets.result.presets;
    if (state.presets.some((p) => p.id === "preset.vhs_tracking")) {
      await insertPreset("preset.vhs_tracking", { x: 0, y: 0 });
    }
    syncCanvasMeta();
    canvas = initCanvas({
      store: { state },
      commands: { ...commands_exports, replaceConnect },
      manifestOf,
      fillThumb,
      onParamChanged: () => requestPreviewSoon(),
      onSelectionChanged: () => {
        render2();
        canvas.render();
        updateStats();
      },
      onDuplicateSelection: duplicateSelection,
      onOpenSearch: (ctx) => overlay.open(ctx),
      onEdgeClick: (edge) => disconnect(edge.toNode, edge.toPort),
      onEdgeDoubleClick: (edge) => insertReroute(edge),
      onViewChanged: () => {
        persistCanvasMeta();
        updateStats();
      },
      onStatus: (m) => setStatus(m, true),
      takePendingLink: () => pendingLink
    });
    library = initLibrary({
      onAdd: (typeId) => addAt(typeId, centerWorld()),
      onInsertPreset: (presetId, pt) => insertPresetAt(presetId, pt || centerWorld())
    });
    inspector = initInspector({
      commands: commands_exports,
      onFocusNode: (id) => canvas.focusNode(id),
      // Every parameter edit must re-render: the engine stores the value, but the
      // image only changes when a new frame is requested.
      onParamChanged: () => requestPreviewSoon(),
      onGraphChanged: () => {
        scheduleRender();
        requestPreviews();
      }
    });
    timeline = initTimeline({
      onSeek: seek,
      onRemoveKeyframe: (lane, k) => {
        const keys = (state.graph?.keyframes?.[lane] || []).filter(
          (x) => !(x.time.numerator === k.time.numerator && x.time.denominator === k.time.denominator)
        );
        setKeyframes(lane, keys);
      }
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
      }
    });
    const g = state.graph;
    if (g) {
      const first = g.nodes.find((n) => n.typeId !== "flux.output");
      if (first) state.selection = /* @__PURE__ */ new Set([first.id]);
    }
    makeTabs();
    render();
    renderAll();
    subscribe((evt) => {
      if (evt === "snapshot") scheduleRender();
    });
    document.getElementById("playBtn").onclick = togglePlay;
    document.getElementById("loopBtn").onclick = toggleLoopMode;
    applyLoopButton();
    document.getElementById("fitBtn").onclick = () => canvas.fit();
    document.getElementById("monitorBtn").onclick = () => {
      openMonitor().then(() => document.getElementById("monitorBtn").classList.add("on")).catch((e) => setStatus("monitor: " + e, true));
    };
    document.getElementById("monitorClose").onclick = () => {
      overlay.toggleMonitor(false);
      document.getElementById("monitorBtn").classList.remove("on");
    };
    document.getElementById("monitorFull").onclick = () => {
      toggleMonitorFullscreen().catch((e) => setStatus("monitor: " + e, true));
    };
    document.getElementById("hudFit").onclick = () => canvas.fit();
    document.getElementById("hudAdd").onclick = () => overlay.open({ world: centerWorld() });
    document.getElementById("undoBtn").onclick = () => undo().then(() => {
      renderAll();
      requestPreviews();
    });
    document.getElementById("redoBtn").onclick = () => redo().then(() => {
      renderAll();
      requestPreviews();
    });
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
      const res = await importMedia(path);
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
      input?.addEventListener("input", () => {
        if (output) output.textContent = input.value;
      });
    }
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
      const pct = total ? Math.round(done / total * 100) : 0;
      setStatus(`exporting\u2026 ${pct}% (${done}/${total} frames)`);
    });
    listen("export-done", (p) => {
      state.exporting = false;
      if (p.video) setStatus(`video \u2192 ${p.video}`);
      else if (p.framesDir) setStatus(`frames \u2192 ${p.framesDir} (no video produced)`);
      else setStatus(`export done: ${p.frames} frames`);
      requestPreviews();
    });
    listen("export-error", (p) => {
      state.exporting = false;
      setStatus("export error: " + (p.message || "unknown"), true);
      requestPreviews();
    });
    let pulling = false;
    const pullSnapshot = () => {
      if (pulling) return;
      pulling = true;
      snapshot().catch(() => {
      }).finally(() => {
        pulling = false;
      });
    };
    listen("live-control", pullSnapshot);
    const vp = document.getElementById("viewport");
    vp.addEventListener("contextmenu", onContextMenu);
    vp.addEventListener("dragover", (ev) => {
      ev.preventDefault();
      ev.dataTransfer.dropEffect = "copy";
    });
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
    requestAnimationFrame(() => {
      canvas.fit();
      requestPreviews();
    });
    setStatus(
      `ready \xB7 ${state.graph?.nodes?.length || 0} nodes \xB7 ${state.catalog.length} node types \xB7 ${state.presets.length} presets`
    );
  }
  var pendingLink = null;
  window.addEventListener("DOMContentLoaded", () => {
    boot().catch((e) => {
      const message = e?.message || String(e);
      console.error("Flux startup failed", e);
      setStatus("startup failed: " + message, true);
      document.title = "Flux | startup error";
    });
  });
})();
