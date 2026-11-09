// Single source of truth for the editor UI: the last snapshot from Rust, the
// catalog, selection, viewport transform and transport state. Modules mutate
// through helpers and re-render via `emit`; nothing here talks to Tauri except
// through `commands.js`.

export const state = {
  revision: 0,
  project: null,
  graph: null,          // active graph object { id, nodes, edges, outputNode }
  activeGraphId: null,
  catalog: [],
  catalogById: new Map(),
  selection: new Set(),  // node ids
  selectedClip: null,
  time: 0,
  playing: false,
  view: { x: 0, y: 0, zoom: 1 },
  frames: [],            // [{ id, title, rect:[x,y,w,h] }]
  presets: [],
  ui: { libTab: "nodes", collapsed: {}, inspector: true, minimap: true, perf: true },
};

const listeners = new Set();
export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}
export function emit(evt = "change") {
  for (const fn of listeners) fn(evt);
}

export function setCatalog(entries) {
  state.catalog = entries;
  state.catalogById = new Map(entries.map((e) => [e.id, e]));
}

export function manifestOf(typeId) {
  return state.catalogById.get(typeId);
}

/** Apply a command response: adopt revision + snapshots. */
export function applyResponse(res) {
  if (!res) return;
  if (res.revision != null) state.revision = res.revision;
  const r = res.result || {};
  if (r.project && r.project.projectId) state.project = r.project;
  if (r.activeGraphId) state.activeGraphId = r.activeGraphId;
  if (r.activeGraph !== undefined) state.graph = r.activeGraph;
  emit("snapshot");
}

/** Rebuild local frames from the project's canvas extension metadata. */
export function syncCanvasMeta() {
  const ext = state.project?.extensions?.["dev.flux.canvas"];
  state.frames = Array.isArray(ext?.frames) ? ext.frames : [];
  if (ext?.viewport) state.view = { ...state.view, ...ext.viewport };
}

export function selectedNodes() {
  if (!state.graph) return [];
  return state.graph.nodes.filter((n) => state.selection.has(n.id));
}
