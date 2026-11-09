// Typed command builders over the engine's GUI surface. Each helper submits one
// command, applies the returned snapshot, and surfaces errors to the status bar.
// A revision conflict triggers one snapshot refetch + retry.

import { dispatch } from "./ipc.js";
import { state, applyResponse, syncCanvasMeta } from "./store.js";

let onStatus = (m) => console.warn(m);
export function setStatusSink(fn) { onStatus = fn; }

let inFlight = Promise.resolve();

/** Serialize commands so the engine never sees overlapping revisions. */
function enqueue(task) {
  const run = inFlight.then(task, task);
  inFlight = run.catch(() => {});
  return run;
}

async function run(command, { expected = state.revision } = {}) {
  const res = await dispatch(command, expected);
  if (res.error) {
    if (res.error.code === "RevisionConflict") {
      // Refetch and retry the intent exactly once with the fresh revision.
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
    return res;
  }
  applyResponse(res);
  await refreshIfNeeded(res);
  syncCanvasMeta();
  return res;
}

/** Most edit handlers return only a revision/id, not a document snapshot. Pull
 *  one so the canvas and inspector always reflect the committed state. */
async function refreshIfNeeded(res) {
  const r = res?.result;
  const hasSnapshot = r && (r.project || r.activeGraph !== undefined);
  if (hasSnapshot) return;
  const snap = await dispatch({ type: "snapshot" }, 0);
  if (!snap.error) {
    applyResponse(snap);
    syncCanvasMeta();
  }
}

// ---- structural graph edits ------------------------------------------------

export function addNode(typeId, x, y) {
  return enqueue(() => run({ type: "addGraphNode", typeId, x, y }));
}
/** Add a fully-specified node (used by duplicate); includes params/position. */
export function addNodeObj(node) {
  return enqueue(() =>
    run({
      type: "apply",
      command: { type: "addNode", graphId: state.activeGraphId, node },
    })
  );
}
export function removeNode(nodeId) {
  return enqueue(() => run({ type: "removeGraphNode", nodeId }));
}
export function removeNodes(nodeIds) {
  return enqueue(async () => {
    let last;
    for (const id of nodeIds) last = await run({ type: "removeGraphNode", nodeId: id });
    return last;
  });
}
export function connect(fromNode, fromPort, toNode, toPort) {
  return enqueue(() => run({ type: "connectPins", fromNode, fromPort, toNode, toPort }));
}
export function disconnect(toNode, toPort) {
  return enqueue(() => run({ type: "disconnectPin", toNode, toPort }));
}
export function moveNode(nodeId, x, y) {
  return enqueue(() => run({ type: "moveNode", nodeId, x, y }));
}
export function moveNodes(list) {
  return enqueue(async () => {
    let last;
    for (const n of list) last = await run({ type: "moveNode", nodeId: n.nodeId, x: n.x, y: n.y });
    return last;
  });
}
export function setBypass(nodeId, bypass) {
  return enqueue(() => run({ type: "setBypass", nodeId, bypass }));
}
export function setGraphOutput(nodeId) {
  return enqueue(() => run({ type: "setGraphOutput", nodeId }));
}
export function newGraph(name) {
  return enqueue(() => run({ type: "newGraph", name }));
}

// ---- parameters -----------------------------------------------------------

export function paramValue(kind, value) {
  if (kind === "bool") return { kind: "bool", value: !!value };
  if (kind === "int" || kind === "enum") return { kind: "int", value: Math.round(Number(value) || 0) };
  if (kind === "color") return { kind: "color", value };
  if (kind === "vec2") return { kind: "vec2", value };
  if (kind === "vec3") return { kind: "vec3", value };
  return { kind: "float", value: Number(value) || 0 };
}

export function setParam(nodeId, paramId, kind, value) {
  const pv = paramValue(kind, value);
  return enqueue(() =>
    run({
      type: "apply",
      command: { type: "setParameter", graphId: state.activeGraphId, nodeId, paramId, value: pv },
    })
  );
}

/** Coalesce rapid slider edits into one command per animation frame. */
const pending = new Map();
let flushScheduled = false;
export function setParamLive(nodeId, paramId, kind, value) {
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

// ---- animation / modulation ----------------------------------------------

export function setKeyframes(laneId, keyframes) {
  return enqueue(() => run({ type: "setKeyframes", laneId, keyframes }));
}
export function setMapping(mapping) {
  return enqueue(() => run({ type: "setMapping", mapping }));
}
export function removeMapping(mappingId) {
  return enqueue(() => run({ type: "removeMapping", mappingId }));
}

// ---- presets --------------------------------------------------------------

export function listPresets() {
  return enqueue(() => run({ type: "presets" }, { expected: 0 }));
}
export function insertPreset(presetId, position, mergeInto = null) {
  // The engine takes a 2-element array; callers often pass a {x, y} world point.
  const pos = Array.isArray(position)
    ? position
    : [Number(position?.x) || 0, Number(position?.y) || 0];
  return enqueue(() =>
    run({ type: "insertPreset", presetId, position: pos, mergeInto })
  );
}

// ---- project lifecycle ----------------------------------------------------

export function undo() { return enqueue(() => run({ type: "undo" })); }
export function redo() { return enqueue(() => run({ type: "redo" })); }
export function snapshot() {
  return enqueue(() => run({ type: "snapshot" }, { expected: 0 }));
}
export function catalog() {
  return enqueue(() => run({ type: "catalog" }, { expected: 0 }));
}
export function createProject(name, seed, width, height, fpsNum, fpsDen) {
  return enqueue(() =>
    run({ type: "createProject", name, seed, width, height, fpsNum, fpsDen }, { expected: 0 })
  );
}
export function saveProject(path) {
  return enqueue(() => run({ type: "save", path }));
}
export function loadProject(json) {
  return enqueue(() => run({ type: "loadProject", json }, { expected: 0 }));
}

/** Persist frames/viewport metadata (not an undoable structural edit). */
export function setCanvasMeta(frames, viewport) {
  return enqueue(() => run({ type: "setCanvasMeta", frames, viewport }));
}
