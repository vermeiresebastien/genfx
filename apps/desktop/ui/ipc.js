// Transport to the Rust engine. Everything (state, validation, rendering) is
// owned by Rust; this module only submits the typed command envelope and reads
// snapshots. No engine logic lives here.

const invoke = window.__TAURI__.core.invoke;

export function rid() {
  try {
    if (crypto && typeof crypto.randomUUID === "function") return crypto.randomUUID();
  } catch (e) {}
  return "r-" + Math.random().toString(36).slice(2) + Date.now().toString(36);
}

/** Low-level dispatch with the protocol + revision envelope. */
export async function dispatch(command, expectedRevision) {
  const req = {
    protocolVersion: 1,
    requestId: rid(),
    projectId: null,
    expectedRevision: expectedRevision ?? 0,
    command,
  };
  return invoke("dispatch", { request: req });
}

/** Render the active composition graph to a PNG data URL. */
export async function renderPreview(timeSeconds, width = 640, height = 360) {
  return invoke("render_preview", { timeSeconds, width, height });
}

/** Render any graph (optionally forcing an output node) to a PNG data URL. */
export async function renderGraphPreview(graphId, timeSeconds, width, height, outputNodeId) {
  return invoke("render_graph_preview", {
    graphId,
    timeSeconds,
    width,
    height,
    outputNodeId: outputNodeId ?? null,
  });
}

export async function exportProject(outDir, width, height) {
  return invoke("export_project_cmd", { outDir, width, height });
}
