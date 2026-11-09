// Node library + presets panel. Categorized, searchable, collapsible, drag to
// the canvas or click to insert at the view centre.

import { state, manifestOf } from "../store.js";
import { categoryColor } from "./node.js";

let filter = "";
let host;
let opts;

const PRIMARY_CATS = [
  "Generators", "Vector", "Typography", "Image animation",
  "3D SDF", "3D Mesh", "Particles", "Physics", "Simulation", "Control",
  "Blur", "Color", "Blend", "Distortion", "Light", "Optical", "Material",
  "Glitch", "Analog", "Temporal", "Matte", "Style", "Texture", "Technical",
  "Print and Pixel", "Utility", "Input/Output", "Mask", "Conditional",
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
  return (
    e.name.toLowerCase().includes(f) ||
    e.id.toLowerCase().includes(f) ||
    (e.help || "").toLowerCase().includes(f) ||
    (e.category || "").toLowerCase().includes(f)
  );
}

function loadCollapsed() {
  try {
    const raw = localStorage.getItem("flux.libCollapsed");
    if (raw) state.ui.collapsed = JSON.parse(raw);
  } catch (e) {}
}
function saveCollapsed() {
  try { localStorage.setItem("flux.libCollapsed", JSON.stringify(state.ui.collapsed)); } catch (e) {}
}

function renderNodesTab() {
  const groups = {};
  for (const e of state.catalog) {
    if (!matches(e)) continue;
    (groups[e.category] ||= []).push(e);
  }
  host.innerHTML = "";
  const cats = orderedCategories(Object.keys(groups));
  if (!cats.length) {
    host.innerHTML = `<div class="lib-empty">No nodes match “${filter}”.</div>`;
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
    nm.textContent = `${collapsed ? "▸" : "▾"} ${cat}`;
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

export function initLibrary(options) {
  opts = options;
  host = document.getElementById("library");
  loadCollapsed();

  const search = document.getElementById("search");
  search.addEventListener("input", () => {
    filter = search.value;
    render();
  });

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

export function render() {
  if (!host) return;
  if (state.ui.libTab === "presets") renderPresetsTab();
  else renderNodesTab();
}

/** First match for the search overlay. */
export function searchNodes(q, limit = 24) {
  const f = q.toLowerCase().trim();
  const list = state.catalog.filter((e) => {
    if (!f) return true;
    return e.name.toLowerCase().includes(f) || e.id.toLowerCase().includes(f) || (e.help || "").toLowerCase().includes(f);
  });
  return list.slice(0, limit);
}
