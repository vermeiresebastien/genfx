// Floating overlays: the spacebar node search, the context menu, and the
// detached preview monitor.

import { searchNodes, render as renderLibrary } from "./library.js";

let opts;
let results = [];
let active = 0;
let pendingContext = null;

export function initOverlay(options) {
  opts = options;

  // ---- search ----
  const overlay = document.getElementById("searchOverlay");
  const input = document.getElementById("searchInput");
  const list = document.getElementById("searchResults");

  function draw() {
    results = searchNodes(input.value, 30);
    active = Math.min(active, Math.max(0, results.length - 1));
    list.innerHTML = "";
    if (!results.length) {
      const d = document.createElement("div");
      d.className = "sr dim";
      d.textContent = "No matches";
      list.appendChild(d);
      return;
    }
    results.forEach((e, i) => {
      const d = document.createElement("div");
      d.className = "sr" + (i === active ? " active" : "");
      const nm = document.createElement("span");
      nm.textContent = e.name;
      const id = document.createElement("span");
      id.className = "dim";
      id.textContent = e.id;
      const cat = document.createElement("span");
      cat.className = "cat";
      cat.textContent = e.category;
      d.append(nm, id, cat);
      d.addEventListener("pointerdown", (ev) => {
        ev.preventDefault();
        choose(i);
      });
      list.appendChild(d);
    });
  }

  function choose(i) {
    const e = results[i];
    if (!e) return;
    close();
    opts.onPick(e.id, pendingContext);
    pendingContext = null;
  }

  function open(ctx) {
    pendingContext = ctx || null;
    active = 0;
    input.value = "";
    overlay.classList.add("open");
    draw();
    input.focus();
  }

  function close() {
    overlay.classList.remove("open");
  }

  input.addEventListener("input", () => { active = 0; draw(); });
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "ArrowDown") { ev.preventDefault(); active = Math.min(results.length - 1, active + 1); draw(); }
    else if (ev.key === "ArrowUp") { ev.preventDefault(); active = Math.max(0, active - 1); draw(); }
    else if (ev.key === "Enter") { ev.preventDefault(); choose(active); }
    else if (ev.key === "Escape") { ev.preventDefault(); close(); }
  });
  overlay.addEventListener("pointerdown", (ev) => {
    if (ev.target === overlay) close();
  });

  // ---- context menu ----
  const menu = document.getElementById("contextMenu");
  function openMenu(clientX, clientY, items) {
    menu.innerHTML = "";
    for (const it of items) {
      const d = document.createElement("div");
      d.className = "mi" + (it.danger ? " danger" : "");
      d.textContent = it.label;
      d.addEventListener("click", () => {
        closeMenu();
        it.fn();
      });
      menu.appendChild(d);
    }
    menu.style.left = clientX + "px";
    menu.style.top = clientY + "px";
    menu.classList.add("open");
  }
  function closeMenu() { menu.classList.remove("open"); }
  window.addEventListener("pointerdown", (ev) => {
    if (!menu.contains(ev.target)) closeMenu();
  }, true);
  window.addEventListener("blur", closeMenu);

  // ---- monitor ----
  const monitor = document.getElementById("monitor");
  const mImg = document.getElementById("monitorImg");
  const mHead = document.getElementById("monitorHead");
  let mDrag = null;
  mHead.addEventListener("pointerdown", (ev) => {
    if (ev.target.tagName === "BUTTON") return;
    const r = monitor.getBoundingClientRect();
    mDrag = { dx: ev.clientX - r.left, dy: ev.clientY - r.top };
    window.addEventListener("pointermove", onMDrag);
    window.addEventListener("pointerup", onMUp);
  });
  function onMDrag(ev) {
    if (!mDrag) return;
    monitor.style.left = ev.clientX - mDrag.dx + "px";
    monitor.style.top = ev.clientY - mDrag.dy + "px";
    monitor.style.right = "auto";
    monitor.style.bottom = "auto";
  }
  function onMUp() {
    window.removeEventListener("pointermove", onMDrag);
    window.removeEventListener("pointerup", onMUp);
    mDrag = null;
  }

  return {
    open, close,
    openMenu, closeMenu,
    toggleMonitor(force) {
      const on = force != null ? force : !monitor.classList.contains("open");
      monitor.classList.toggle("open", on);
      return on;
    },
    setMonitorImage(src) { mImg.src = src; },
    isMonitorOpen: () => monitor.classList.contains("open"),
  };
}
