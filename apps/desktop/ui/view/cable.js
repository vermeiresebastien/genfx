// Cable geometry. Cables live in the transformed world container, so all
// coordinates here are world units.

export function bezier(a, b) {
  // Horizontal tangents with a minimum bow so short links stay readable.
  const dx = Math.max(46, Math.abs(b.x - a.x) * 0.5);
  return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
}

export const PORT_TYPES = {
  texture2d: { color: "#6fb1ff", w: 2.5 },
  mask: { color: "#f0b429", w: 2 },
  depth: { color: "#8ae6c8", w: 2 },
  normal: { color: "#c58bff", w: 2 },
  color: { color: "#ff9f6b", w: 2 },
  scalar: { color: "#9ad0ff", w: 1.6 },
  vector_field: { color: "#c0e06a", w: 1.6 },
  path_set: { color: "#e6a8ff", w: 1.6 },
};

export function portStyle(type) {
  return PORT_TYPES[type] || { color: "var(--cable)", w: 2 };
}

/** Port types that can legally connect. Exact types match, and image ports
 *  (texture2d / mask) are interchangeable — the engine binds both as textures. */
export function portsCompatible(fromType, toType) {
  if (!fromType || !toType) return false;
  if (fromType === toType) return true;
  const imagey = new Set(["texture2d", "mask"]);
  return imagey.has(fromType) && imagey.has(toType);
}
