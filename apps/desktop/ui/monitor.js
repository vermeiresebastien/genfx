const invoke = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
if (typeof invoke !== "function") throw new Error("Tauri IPC bridge is unavailable");
const canvas = document.getElementById("frame");
const ctx = canvas.getContext("2d", { alpha: false });
const stateEl = document.getElementById("state");
const info = document.getElementById("info");
let graphId = null;
let time = 0;
let playing = true;
let busy = false;
let last = performance.now();
let lastSync = 0;

function request(command) {
  return invoke("dispatch", { request: {
    protocolVersion: 1,
    requestId: crypto.randomUUID(),
    projectId: null,
    expectedRevision: 0,
    command,
  }});
}

async function syncGraph() {
  const res = await request({ type: "snapshot" });
  graphId = res?.result?.activeGraphId || null;
  stateEl.textContent = graphId ? "live" : "waiting for composition";
}

async function renderFrame() {
  if (busy) return;
  busy = true;
  try {
    if (!graphId || performance.now() - lastSync > 500) {
      await syncGraph();
      lastSync = performance.now();
    }
    if (!graphId) return;
    const buf = await invoke("render_preview_raw", {
      graphId, timeSeconds: time, width: canvas.width, height: canvas.height, outputNodeId: null,
    });
    const u8 = buf instanceof Uint8Array ? buf : Uint8Array.from(buf);
    ctx.putImageData(new ImageData(new Uint8ClampedArray(u8), canvas.width, canvas.height), 0, 0);
    const now = performance.now();
    if (playing) time += Math.min(0.1, (now - last) / 1000);
    last = now;
  } catch (e) {
    stateEl.textContent = "preview error";
    info.textContent = String(e?.message || e);
    graphId = null;
  } finally {
    busy = false;
    requestAnimationFrame(renderFrame);
  }
}

document.getElementById("pause").onclick = (ev) => {
  playing = !playing;
  ev.currentTarget.textContent = playing ? "Pause" : "Play";
};
document.getElementById("full").onclick = () => invoke("toggle_monitor_fullscreen")
  .catch((e) => { info.textContent = String(e); });
window.addEventListener("focus", () => syncGraph().catch(() => {}));
renderFrame();
