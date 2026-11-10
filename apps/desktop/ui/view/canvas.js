// The patch canvas: world transform (pan/zoom), node cards, SVG cables and all
// pointer interactions (move, connect, marquee, context menu). Cables live in
// the same transformed container as the nodes, so pan/zoom needs no extra math.

import { bezier, portStyle, portsCompatible } from "./cable.js";
import {
  NODE_W,
  createNodeEl,
  nodeRect,
  planLayout,
  portPoint,
} from "./node.js";

const SVG_ORIGIN = 5000; // inner <g> offset so negative world coords still draw
const MIN_ZOOM = 0.2;
const MAX_ZOOM = 2.2;

export function initCanvas(opts) {
  const viewport = document.getElementById("viewport");
  const world = document.getElementById("world");
  const svg = document.getElementById("edges");
  const edgeG = document.getElementById("edgeLayer");
  const marquee = document.getElementById("marquee");
  const minimap = document.getElementById("minimap");

  const nodeEls = new Map(); // nodeId -> { el, sig }
  let view = opts.store.state.view;
  let drag = null;    // { ids, starts:Map, originWorld, moved }
  let pan = null;     // { sx, sy, vx, vy }
  let linking = null; // { node, port, dir, ptype, tmp }
  let marq = null;

  const store = opts.store;
  const graph = () => store.state.graph;

  // ---- coordinate helpers -------------------------------------------------
  function viewportRect() { return viewport.getBoundingClientRect(); }
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

  // ---- edge + node rendering ---------------------------------------------
  function edgeType(fromNode, fromPort) {
    const m = opts.manifestOf(fromNode.typeId);
    const p = (m?.outputs || []).find((x) => x.id === fromPort);
    return p?.type;
  }

  function renderEdges() {
    const g = graph();
    edgeG.innerHTML = "";
    if (!g) return;
    const flow = g.nodes.some((n) => {
      const m = opts.manifestOf(n.typeId);
      return m?.category === "Temporal" || m?.generator === false && n.typeId.includes("feedback");
    });
    g.edges.forEach((e, idx) => {
      const from = g.nodes.find((n) => n.id === e.fromNode);
      const to = g.nodes.find((n) => n.id === e.toNode);
      if (!from || !to) return;
      const a = portPoint(from, opts.manifestOf(from.typeId), "out", e.fromPort);
      const b = portPoint(to, opts.manifestOf(to.typeId), "in", e.toPort);
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
        opts.onEdgeClick(e);
      });
      path.addEventListener("dblclick", (ev) => {
        ev.stopPropagation();
        opts.onEdgeDoubleClick(e, idx);
      });
      edgeG.appendChild(path);
    });
  }

  function nodeSignature(node) {
    const m = opts.manifestOf(node.typeId);
    const conn = (graph()?.edges || [])
      .filter((e) => e.fromNode === node.id || e.toNode === node.id)
      .map((e) => `${e.fromNode}:${e.fromPort}>${e.toNode}:${e.toPort}`)
      .join("|");
    const inlineVals = (m?.parameters || [])
      .filter((p) => p.kind === "float" || p.kind === "int" || p.kind === "bool")
      .slice(0, 2)
      .map((p) => {
        const ov = node.params?.[p.id];
        return `${p.id}=${ov !== undefined ? JSON.stringify(ov.value) : JSON.stringify(p.default)}`;
      })
      .join(",");
    return [
      node.typeId,
      node.bypass ? 1 : 0,
      node.missingType ? 1 : 0,
      store.state.selection.has(node.id) ? 1 : 0,
      conn,
      inlineVals,
      m ? 1 : 0,
    ].join("#");
  }

  const cb = {
    manifestOf: opts.manifestOf,
    isSelected: (id) => store.state.selection.has(id),
    isConnected: (nodeId, portId, dir) =>
      (graph()?.edges || []).some((e) =>
        dir === "in" ? e.toNode === nodeId && e.toPort === portId : e.fromNode === nodeId && e.fromPort === portId
      ),
    paramValue: (node, decl) => {
      const ov = node.params?.[decl.id];
      return ov !== undefined ? ov.value : decl.default;
    },
    callbacks: {
      onHeaderDown: (ev, node) => startNodeDrag(ev, node),
      onNodeDown: (ev, node) => selectNode(ev, node),
      onPortDown: (ev, node, port, dir) => startLink(ev, node, port, dir),
      // Connection completion is handled by the window-level pointerup in
      // onLinkUp (elementFromPoint resolves the target port), so the per-port
      // handler is intentionally a no-op.
      onPortUp: () => {},
      onInlineParam: (nodeId, paramId, kind, value) => {
        opts.commands.setParamLive(nodeId, paramId, kind, value);
        opts.onParamChanged?.();
      },
    },
  };

  function renderNodes() {
    const g = graph();
    if (!g) return;
    const seen = new Set();
    for (const node of g.nodes) {
      seen.add(node.id);
      const sig = nodeSignature(node);
      const existing = nodeEls.get(node.id);
      if (existing && existing.sig === sig) {
        existing.el.style.left = node.position[0] + "px";
        existing.el.style.top = node.position[1] + "px";
        continue;
      }
      const el = createNodeEl(node, cb);
      if (existing) existing.el.replaceWith(el);
      else world.appendChild(el);
      nodeEls.set(node.id, { el, sig });
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
      const el = nodeEls.get(node.id)?.el;
      const cv = el?.querySelector("canvas.thumb");
      if (cv) opts.fillThumb(cv, node.id);
    }
  }

  /** Lightweight position update during a drag (no DOM rebuild). */
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
    for (const el of [...world.querySelectorAll(".frame")]) el.remove();
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

  function render() {
    applyTransform();
    renderFrames();
    renderEdges();
    renderNodes();
    drawMinimap();
  }

  // ---- selection ----------------------------------------------------------
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
    opts.onSelectionChanged();
  }

  function selectAllConnected(nodeId) {
    const g = graph();
    if (!g) return;
    const seen = new Set([nodeId]);
    const queue = [nodeId];
    while (queue.length) {
      const id = queue.pop();
      for (const e of g.edges) {
        if (e.fromNode === id && !seen.has(e.toNode)) { seen.add(e.toNode); queue.push(e.toNode); }
        if (e.toNode === id && !seen.has(e.fromNode)) { seen.add(e.fromNode); queue.push(e.fromNode); }
      }
    }
    store.state.selection = seen;
    opts.onSelectionChanged();
  }

  // ---- node drag ----------------------------------------------------------
  function startNodeDrag(ev, node) {
    if (ev.button !== 0) return;
    ev.stopPropagation();
    ev.preventDefault();
    const s = store.state.selection;
    if (!s.has(node.id)) {
      s.clear();
      s.add(node.id);
      opts.onSelectionChanged();
    }
    if (ev.altKey) {
      opts.onDuplicateSelection(Array.from(s));
      return;
    }
    const g = graph();
    if (!g) return;
    const starts = new Map();
    const ids = [];
    for (const id of s) {
      const n = g.nodes.find((x) => x.id === id);
      if (n) { starts.set(id, [n.position[0], n.position[1]]); ids.push(id); }
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
      const list = drag.ids
        .map((id) => {
          const n = g.nodes.find((x) => x.id === id);
          return n ? { nodeId: id, x: n.position[0], y: n.position[1] } : null;
        })
        .filter(Boolean);
      if (list.length) opts.commands.moveNodes(list);
    }
    drag = null;
  }

  // ---- connect ------------------------------------------------------------
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
    const a = portPoint(linking.node, opts.manifestOf(linking.node.typeId), linking.dir, linking.port.id);
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

    const el = document.elementFromPoint(ev.clientX, ev.clientY);
    const target = el && el.closest ? el.closest(".port") : null;
    if (!target) {
      // Dropped on empty canvas: ask for a compatible node and remember the link.
      const w = toWorld(ev.clientX, ev.clientY);
      opts.onOpenSearch({ world: w, link: src });
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
      opts.onStatus(`Incompatible ports: ${outSide.port.type} → ${inSide.port.type}`);
      return;
    }
    opts.commands.replaceConnect(
      outSide.node.id,
      outSide.port.id,
      inSide.node.id,
      inSide.port.id
    );
  }

  /** Called after inserting a node from the search-with-link flow. */
  function completePendingLink(newNodeId) {
    const pending = opts.takePendingLink?.();
    if (!pending) return;
    const t = (typeId) => opts.manifestOf(typeId);
    if (pending.dir === "out") {
      // Connect to the new node's first input of the *same* type. Never fall back
      // to an incompatible port (that produced engine port-type-mismatch errors).
      const newM = t(pending.meta.typeId);
      const inPort = (newM?.inputs || []).find((p) => portsCompatible(pending.ptype, p.type));
      if (inPort) {
        opts.commands.replaceConnect(pending.node.id, pending.port.id, newNodeId, inPort.id);
      } else {
        opts.onStatus(`No ${pending.ptype} input on ${pending.meta.typeId}`, true);
      }
    } else {
      const newM = t(pending.meta.typeId);
      const outPort = (newM?.outputs || []).find((p) => portsCompatible(p.type, pending.ptype)) || null;
      if (outPort) {
        opts.commands.replaceConnect(newNodeId, outPort.id, pending.node.id, pending.port.id);
      } else {
        opts.onStatus(`No ${pending.ptype} output on ${pending.meta.typeId}`, true);
      }
    }
  }

  // ---- marquee + pan ------------------------------------------------------
  function onViewportPointerDown(ev) {
    if (ev.target.closest && (ev.target.closest(".node") || ev.target.closest(".frame"))) return;
    if (ev.button === 1 || (ev.button === 0 && ev.altKey && !ev.target.closest(".port"))) {
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
    // Convert both ends to coords local to the viewport (the marquee element's
    // offset parent), not raw client coords.
    const a = { x: (marq.start.x - view.x) * view.zoom, y: (marq.start.y - view.y) * view.zoom };
    const b = { x: ev.clientX - r.left, y: ev.clientY - r.top };
    marquee.style.left = Math.min(a.x, b.x) + "px";
    marquee.style.top = Math.min(a.y, b.y) + "px";
    marquee.style.width = Math.abs(b.x - a.x) + "px";
    marquee.style.height = Math.abs(b.y - a.y) + "px";
  }

  function onMarqueeMove(ev) { if (marq) updateMarquee(ev); }

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
      h: Math.abs(end.y - marq.start.y),
    };
    const g = graph();
    if (g && (r.w > 4 || r.h > 4)) {
      if (!marq.add) store.state.selection.clear();
      for (const n of g.nodes) {
        const b = nodeRect(n, opts.manifestOf(n.typeId));
        if (b.x < r.x + r.w && b.x + b.w > r.x && b.y < r.y + r.h && b.y + b.h > r.y) {
          store.state.selection.add(n.id);
        }
      }
      opts.onSelectionChanged();
    } else if (!marq.add) {
      // A plain click on empty canvas clears the selection.
      store.state.selection.clear();
      opts.onSelectionChanged();
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
      zoom: view.zoom,
    });
  }
  function onPanEnd() {
    window.removeEventListener("pointermove", onPanMove);
    window.removeEventListener("pointerup", onPanEnd);
    viewport.classList.remove("panning");
    pan = null;
    opts.onViewChanged?.();
  }

  function onWheel(ev) {
    ev.preventDefault();
    const r = viewportRect();
    const mx = ev.clientX - r.left;
    const my = ev.clientY - r.top;
    const wx = view.x + mx / view.zoom;
    const wy = view.y + my / view.zoom;
    const factor = Math.exp(-ev.deltaY * 0.0015);
    const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, view.zoom * factor));
    setView({ x: wx - mx / zoom, y: wy - my / zoom, zoom });
    opts.onViewChanged?.();
  }

  // ---- fit / focus --------------------------------------------------------
  function bounds() {
    const g = graph();
    if (!g || !g.nodes.length) return null;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const n of g.nodes) {
      const rect = nodeRect(n, opts.manifestOf(n.typeId));
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
    if (!b) { setView({ x: 0, y: 0, zoom: 1 }); return; }
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
    const rect = nodeRect(n, opts.manifestOf(n.typeId));
    const zoom = Math.max(view.zoom, 0.9);
    setView({
      x: rect.x + rect.w / 2 - r.width / (2 * zoom),
      y: rect.y + rect.h / 2 - r.height / (2 * zoom),
      zoom,
    });
  }

  function centerOn(worldPt) {
    const r = viewportRect();
    setView({ x: worldPt.x - r.width / (2 * view.zoom), y: worldPt.y - r.height / (2 * view.zoom), zoom: view.zoom });
  }

  // ---- minimap ------------------------------------------------------------
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
      const rect = nodeRect(n, opts.manifestOf(n.typeId));
      ctx.fillStyle = store.state.selection.has(n.id) ? "#4a9eff" : "#3a4656";
      ctx.fillRect(ox + rect.x * s, oy + rect.y * s, Math.max(2, rect.w * s), Math.max(2, rect.h * s));
    }
    // viewport rectangle
    const vr = viewportRect();
    ctx.strokeStyle = "#8fc4ff";
    ctx.lineWidth = 1;
    ctx.strokeRect(ox + view.x * s, oy + view.y * s, (vr.width / view.zoom) * s, (vr.height / view.zoom) * s);
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

  // ---- wiring -------------------------------------------------------------
  viewport.addEventListener("pointerdown", onViewportPointerDown);
  viewport.addEventListener("wheel", onWheel, { passive: false });
  minimap.addEventListener("pointerdown", minimapClick);

  applyTransform();
  return {
    render,
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
    drawMinimap,
  };
}
