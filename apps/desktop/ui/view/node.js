// Node card construction + geometry. Nodes are DOM (crisp text, inline controls,
// native inputs); cables are SVG. All coordinates are world units.

export const NODE_W = 176;
export const HDR_H = 28;
export const ROW_H = 20;

const CAT_COLORS = {
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
  Technical: "#8fa0b0",
};

export function categoryColor(category) {
  return CAT_COLORS[category] || "#6fb1ff";
}

function portClass(port) {
  if (port.type === "mask" || port.id === "mask") return "mask";
  if (port.id === "b") return "aux";
  return "";
}

/** Numeric params small enough to edit inline on the node card. */
export function inlineParams(manifest) {
  const params = (manifest?.parameters || []).filter(
    (p) => (p.kind === "float" || p.kind === "int" || p.kind === "bool") && !p.hidden
  );
  // Keep it to two so the card stays compact; prefer non-wet first?
  return params.slice(0, 2);
}

export function planLayout(node, manifest) {
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

export function portY(plan, index) {
  return HDR_H + 4 + index * ROW_H + ROW_H / 2;
}

export function portPoint(node, manifest, dir, portId) {
  const plan = planLayout(node, manifest);
  const ports = dir === "in" ? plan.inputs : plan.outputs;
  const idx = Math.max(0, ports.findIndex((p) => p.id === portId));
  const x = node.position[0] + (dir === "in" ? 0 : NODE_W);
  const y = node.position[1] + portY(plan, idx);
  return { x, y };
}

export function nodeRect(node, manifest) {
  const plan = planLayout(node, manifest);
  return { x: node.position[0], y: node.position[1], w: NODE_W, h: plan.h };
}

/**
 * Build a node card.
 * opts: { manifestOf, isSelected, isConnected, paramValue, callbacks }
 */
export function createNodeEl(node, opts) {
  const m = opts.manifestOf(node.typeId);
  const plan = planLayout(node, m);
  const el = document.createElement("div");
  el.className =
    "node" +
    (opts.isSelected(node.id) ? " sel" : "") +
    (node.bypass ? " bypassed" : "") +
    (node.missingType ? " missing" : "");
  el.style.left = node.position[0] + "px";
  el.style.top = node.position[1] + "px";
  el.dataset.nodeId = node.id;
  el.style.setProperty("--cat", categoryColor(m?.category || "Utility"));

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
  hdr.title = `${m?.name ?? node.typeId}${m?.help ? " — " + m.help : ""}`;
  hdr.addEventListener("pointerdown", (ev) => opts.callbacks.onHeaderDown(ev, node));
  el.appendChild(hdr);

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
      if (opts.isConnected(node.id, p.id, "in")) d.classList.add("connected");
      d.dataset.node = node.id;
      d.dataset.port = p.id;
      d.dataset.dir = "in";
      d.dataset.ptype = p.type;
      d.title = `input ${p.id} (${p.type})${p.required ? " · required" : ""}`;
      d.addEventListener("pointerdown", (ev) => opts.callbacks.onPortDown(ev, node, p, "in"));
      d.addEventListener("pointerup", (ev) => opts.callbacks.onPortUp(ev, node, p, "in"));
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
      if (opts.isConnected(node.id, p.id, "out")) d.classList.add("connected");
      d.dataset.node = node.id;
      d.dataset.port = p.id;
      d.dataset.dir = "out";
      d.dataset.ptype = p.type;
      d.title = `output ${p.id} (${p.type})`;
      d.addEventListener("pointerdown", (ev) => opts.callbacks.onPortDown(ev, node, p, "out"));
      d.addEventListener("pointerup", (ev) => opts.callbacks.onPortUp(ev, node, p, "out"));
      row.appendChild(d);
    }
    ports.appendChild(row);
  }
  el.appendChild(ports);

  if (plan.inline.length) {
    const box = document.createElement("div");
    box.className = "inline";
    for (const decl of plan.inline) {
      const val = opts.paramValue(node, decl);
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
        opts.callbacks.onInlineParam(node.id, decl.id, decl.kind, v);
      });
      wrap.appendChild(input);
      box.appendChild(wrap);
    }
    el.appendChild(box);
  }

  if (node.typeId === "flux.output") {
    // A canvas is painted from raw RGBA (no PNG encode/decode per frame) and is
    // sized to device pixels so the thumbnail stays sharp when zoomed.
    const canvas = document.createElement("canvas");
    canvas.className = "thumb";
    canvas.dataset.thumbFor = node.id;
    const cssW = NODE_W;
    const dpr = Math.min(2, globalThis.devicePixelRatio || 1);
    canvas.width = Math.round(cssW * dpr);
    canvas.height = Math.round((cssW * 9 / 16) * dpr);
    canvas.addEventListener("pointerdown", (ev) => ev.stopPropagation());
    el.appendChild(canvas);
  }

  el.addEventListener("pointerdown", (ev) => {
    if (ev.target.classList.contains("port")) return;
    opts.callbacks.onNodeDown(ev, node);
  });
  return el;
}
