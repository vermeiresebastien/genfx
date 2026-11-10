// Transport to the Rust engine. Everything (state, validation, rendering) is
// owned by Rust; this module only submits the typed command envelope and reads
// snapshots. No engine logic lives here.

// Tauri 2 may expose the bridge through the compatibility global or the
// internal runtime global depending on the packaged WebView configuration.
// Resolve both forms before importing any UI module so production startup
// cannot fail before boot() runs.
const invoke = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
if (typeof invoke !== "function") {
  throw new Error("Tauri IPC bridge is unavailable");
}

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

/** Render a graph to raw RGBA (binary IPC, no PNG/base64). Fast path. */
export async function renderPreviewRaw(graphId, timeSeconds, width, height, outputNodeId) {
  const buf = await invoke("render_preview_raw", {
    graphId,
    timeSeconds,
    width,
    height,
    outputNodeId: outputNodeId ?? null,
  });
  const u8 =
    buf instanceof Uint8Array
      ? buf
      : buf instanceof ArrayBuffer
      ? new Uint8Array(buf)
      : Uint8Array.from(buf);
  return { w: width, h: height, u8 };
}

export async function buildInfo() {
  try { return await invoke("build_info"); } catch (e) { return null; }
}

export async function exportProject(outDir, width, height, codec = "h264", keepFrames = false) {
  return invoke("export_project_cmd", { outDir, width, height, codec, keepFrames });
}

/** Subscribe to a Tauri event; returns the unlisten promise. */
export function listen(event, handler) {
  const ev = window.__TAURI__?.event;
  if (!ev || !ev.listen) return Promise.resolve(() => {});
  return ev.listen(event, (e) => handler(e.payload));
}
