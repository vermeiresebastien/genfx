// Secondary timeline: keyframe lanes, playback range and the playhead. Collapsed
// by default — the graph is the primary surface, this is for timing.

import { state, manifestOf } from "../store.js";

let opts;
let host, head;

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
  return `${m?.name ?? nodeId} · ${label}`;
}

export function initTimeline(options) {
  opts = options;
  host = document.getElementById("timeline");
  head = document.getElementById("timelineHead");
  render();
  return { render };
}

export function render() {
  if (!host) return;
  host.innerHTML = "";
  const g = state.graph;
  if (!g) {
    host.appendChild(el("div", "dim", "No composition."));
    return;
  }
  const dur = durationSeconds();
  const lanes = Object.entries(g.keyframes || {}).filter(([, keys]) => keys.length);

  // Range/playhead bar
  const bar = el("div", "tl-bar");
  const playhead = el("div", "tl-play");
  playhead.style.left = `${(Math.min(state.time, dur) / dur) * 100}%`;
  bar.appendChild(playhead);
  bar.addEventListener("pointerdown", (ev) => {
    const r = bar.getBoundingClientRect();
    const t = Math.max(0, Math.min(dur, ((ev.clientX - r.left) / r.width) * dur));
    opts.onSeek(t);
  });
  host.appendChild(bar);

  if (!lanes.length) {
    host.appendChild(el("div", "dim", "No keyframes. Use ◆ on a parameter to animate it."));
    return;
  }
  for (const [lane, keys] of lanes) {
    const row = el("div", "tl-lane");
    const name = el("span", "tl-name", laneLabel(lane));
    row.appendChild(name);
    const track = el("div", "tl-track");
    for (const k of keys) {
      const t = Number(k.time.numerator) / k.time.denominator;
      const d = el("div", "tl-key");
      d.style.left = `${Math.min(100, (t / dur) * 100)}%`;
      d.title = `${t.toFixed(3)}s`;
      d.addEventListener("pointerdown", (ev) => {
        ev.stopPropagation();
        opts.onSeek(t);
      });
      d.addEventListener("dblclick", (ev) => {
        ev.stopPropagation();
        opts.onRemoveKeyframe(lane, k);
      });
      track.appendChild(d);
    }
    row.appendChild(track);
    host.appendChild(row);
  }
}

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}
