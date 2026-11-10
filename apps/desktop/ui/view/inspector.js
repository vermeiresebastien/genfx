// Contextual inspector: parameter editors, connections, animation/modulation
// and node actions. The node card stays readable on its own; this panel is the
// deep-dive for the selected node.

import { state, manifestOf } from "../store.js";
import * as commands from "../commands.js";
import { categoryColor } from "./node.js";

let opts;

export function initInspector(options) {
  opts = options;
  render();
  return { render };
}

/** Apply a parameter edit and request a fresh frame. */
function commitParam(node, decl, kind, value) {
  commands.setParam(node.id, decl.id, kind, value);
  opts?.onParamChanged?.();
}

function timeWire(seconds) {
  const micros = Math.round(seconds * 1_000_000);
  return { numerator: String(micros), denominator: 1_000_000 };
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
  if (ov !== undefined) return ov.value;
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

// ---- param controls -------------------------------------------------------

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
  const update = () => { readout.value = format(input.value); readout.textContent = format(input.value); };
  input.addEventListener("input", () => { update(); onInput(Number(input.value)); });
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
  // Enums are written as Int index: the router reads ParamValue::as_f64(), and
  // ParamValue::Enum (a string) resolves to 0.
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
  input.placeholder = "Enter text…";
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
      commands.setParam(node.id, decl.id, "color", v);
      opts?.onParamChanged?.();
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
      commands.setParam(node.id, decl.id, decl.kind, v);
      opts?.onParamChanged?.();
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
    b.textContent = "◆";
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
    b.textContent = "∿";
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
    const value = commands.paramValue(decl.kind, valueOf(node, decl));
    list.push({ time: timeWire(state.time), value, interpolation: "linear" });
    list.sort((a, b) => Number(a.time.numerator) / a.time.denominator - Number(b.time.numerator) / b.time.denominator);
  }
  commands.setKeyframes(lane, list).then(() => opts?.onGraphChanged?.());
}

function toggleModulation(node, decl, existing) {
  if (existing) {
    commands.removeMapping(existing.id).then(() => opts?.onGraphChanged?.());
    return;
  }
  const id = "map-" + Math.random().toString(36).slice(2) + Date.now().toString(36);
  commands.setMapping({
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
    enabled: true,
  }).then(() => opts?.onGraphChanged?.());
}

function paramRow(node, decl) {
  switch (decl.kind) {
    case "bool": return boolRow(node, decl);
    case "enum": return enumRow(node, decl);
    case "text": return textRow(node, decl);
    case "color": return colorRow(node, decl);
    case "vec2":
    case "vec3": return vecRow(node, decl);
    default: return numericRow(node, decl);
  }
}

// ---- sections -------------------------------------------------------------

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

function renderEmpty() {
  const host = document.getElementById("inspector");
  const g = state.graph;
  host.innerHTML = "";
  host.appendChild(el("h2", null, g?.name || "Composition"));
  if (!g) {
    host.appendChild(el("div", "muted", "No graph."));
    return;
  }
  host.appendChild(el("div", "muted", `${g.nodes.length} nodes · ${g.edges.length} cables`));
  host.appendChild(el("hr"));
  host.appendChild(el("div", "muted", "Select a node to edit its parameters."));
  const hints = el("div", "dim");
  hints.style.marginTop = "10px";
  hints.textContent =
    "Space: add node · drag a port: connect · Tab: inspector · 1: fit · 2: fit selected";
  host.appendChild(hints);
}

function renderMulti(nodes) {
  const host = document.getElementById("inspector");
  host.innerHTML = "";
  host.appendChild(el("h2", null, `${nodes.length} nodes selected`));
  host.appendChild(el("hr"));
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
  host.appendChild(align);
  const row = el("div", "btnrow");
  const del = mk("Delete", () => opts.commands.removeNodes(nodes.map((n) => n.id)));
  del.style.color = "var(--err)";
  row.appendChild(del);
  host.appendChild(row);
}

function alignNodes(nodes, mode) {
  if (mode === "left") {
    const x = Math.min(...nodes.map((n) => n.position[0]));
    commands.moveNodes(nodes.map((n) => ({ nodeId: n.id, x, y: n.position[1] })));
  } else {
    const y = Math.min(...nodes.map((n) => n.position[1]));
    commands.moveNodes(nodes.map((n) => ({ nodeId: n.id, x: n.position[0], y })));
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
    case "bool": return Math.random() >= 0.5;
    case "enum": return Math.floor(unit() * Math.max(1, (decl.options || []).length));
    case "text": return ["FLUX", "GENFX", "Hello world", "VIDEO", "0101"][Math.floor(unit() * 5)];
    case "color": return [unit(), unit(), unit(), 1];
    case "vec2": return [range(), range()];
    case "vec3": return [range(), range(), range()];
    case "int": return Math.round(range());
    default: return range();
  }
}

async function randomizeNode(node, decls) {
  await Promise.all(decls.map((d) => opts.commands.setParam(node.id, d.id, d.kind, randomizeValue(d))));
  opts?.onParamChanged?.();
}

function renderSingle(node) {
  const host = document.getElementById("inspector");
  const m = manifestOf(node.typeId);
  host.innerHTML = "";

  const h = el("h2");
  const dot = el("span");
  dot.style.cssText = `display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:6px;background:${categoryColor(m?.category || "Utility")}`;
  h.append(dot, document.createTextNode(m?.name ?? node.typeId));
  host.appendChild(h);
  host.appendChild(el("div", "muted", `${node.typeId} · ${m?.category || "?"}${m?.kind ? " · " + m.kind : ""}`));
  if (m?.help) {
    const help = el("div", "dim");
    help.style.marginTop = "4px";
    help.textContent = m.help;
    host.appendChild(help);
  }
  if (node.missingType) host.appendChild(el("div", null, "⚠ type unavailable")).style.color = "var(--err)";
  host.appendChild(el("hr"));

  // Parameters
  const decls = (m?.parameters || []).filter((d) => !d.hidden);
  host.appendChild(el("div", "sec", "Parameters"));
  if (!decls.length) {
    host.appendChild(el("div", "muted", "No editable parameters."));
  }
  for (const d of decls) host.appendChild(paramRow(node, d));

  // Connections
  host.appendChild(el("div", "sec", "Connections"));
  const g = state.graph;
  const list = el("div", "conn-list");
  const inputs = m?.inputs || [];
  if (!inputs.length) list.appendChild(el("div", "dim", "No inputs."));
  for (const p of inputs) {
    const e = g.edges.find((x) => x.toNode === node.id && x.toPort === p.id);
    const row = el("div", "ci");
    const pp = el("span", "pp", p.id);
    const txt = el("span", null, e ? `← ${nameOf(e.fromNode)}:${e.fromPort}` : `— not connected${p.required ? " (required)" : ""}`);
    row.append(pp, txt);
    if (e) row.addEventListener("click", () => opts.onFocusNode(e.fromNode));
    list.appendChild(row);
  }
  host.appendChild(list);

  // Actions
  host.appendChild(el("div", "sec", "Actions"));
  const row = el("div", "btnrow");
  const byp = el("button", node.bypass ? "on" : null, node.bypass ? "Bypassed" : "Bypass");
  byp.addEventListener("click", () => {
    commands.setBypass(node.id, !node.bypass);
    opts?.onGraphChanged?.();
  });
  row.appendChild(byp);

  const reset = el("button", null, "Reset");
  reset.addEventListener("click", () => {
    for (const d of decls) commands.setParam(node.id, d.id, d.kind, d.default);
    opts?.onParamChanged?.();
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
      commands.setGraphOutput(node.id);
      opts?.onGraphChanged?.();
    });
    row.appendChild(setOut);
  }
  host.appendChild(row);

  const row2 = el("div", "btnrow");
  const del = el("button", null, "Delete node");
  del.style.color = "var(--err)";
  del.addEventListener("click", () => {
    state.selection.clear();
    commands.removeNode(node.id);
  });
  row2.appendChild(del);
  host.appendChild(row2);
}

function nameOf(nodeId) {
  const n = state.graph?.nodes.find((x) => x.id === nodeId);
  if (!n) return nodeId;
  const m = manifestOf(n.typeId);
  return m?.name ?? n.typeId;
}

export function render() {
  const host = document.getElementById("inspector");
  if (!host) return;
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
