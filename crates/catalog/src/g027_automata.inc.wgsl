// Cellular automata presentation: the state grid is evolved by the CPU
// fixed-step scheduler and uploaded as `src_tex`. This shader adds the styled
// live-cell glow and respects a generated deterministic state when the
// scheduler runs in "shader" mode.
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let cs = max(p_cellSize(), 1.0);
  let g = uv * vec2<f32>(frame.dims.xy) / cs;
  let cell = floor(g);
  // Deterministic initial state from seed and a synthetic evolution (bounded),
  // used for generators that present without an uploaded simulation buffer.
  var alive = 0.0;
  let kind = p_kind();
  if (kind == 1u) {
    // Elementary rule: 1D strip over y.
    let x = u32(cell.x) % 256u;
    let y = floor(time_seconds() * 30.0);
    let rule = u32(p_rule());
    var state = (hash21(vec2<f32>(f32(x), f32(seedf()) * 100.0)) > 0.5);
    var idx = u32(0);
    let yy = u32(u32(y) % 64u);
    var bits = state;
    var local = 0.0;
    let l = fract(g.x);
    var cur = state;
    var stepse = u32(0);
    while (stepse < yy) {
      let left = hash21(vec2<f32>(f32(x) - 1.0, f32(stepse)));
      let right = hash21(vec2<f32>(f32(x) + 1.0, f32(stepse)));
      let lv = select(0u, 1u, left > 0.5);
      let rv = select(0u, 1u, right > 0.5);
      let cv = select(0u, 1u, cur);
      let pat = lv * 4u + cv * 2u + rv;
      cur = ((rule >> pat) & 1u) == 1u;
      stepse = stepse + 1u;
    }
    alive = select(0.0, 1.0, cur && fract(g.y) < 0.7);
  } else if (kind == 2u) {
    let s = u32(hash21(cell) * 8.0);
    let phase = u32(floor(time_seconds() * 4.0));
    alive = f32((s + phase) % 8u) / 8.0;
  } else {
    // Life-like glow: settle patterns from the seeded state.
    var live = hash21(cell + vec2<f32>(floor(time_seconds() * 4.0))) > 0.35;
    var total = 0;
    for (var oy = -1; oy <= 1; oy = oy + 1) {
      for (var ox = -1; ox <= 1; ox = ox + 1) {
        if (ox != 0 || oy != 0) {
          total = total + i32(hash21(cell + vec2<f32>(f32(ox), f32(oy)) + vec2<f32>(floor(time_seconds() * 4.0))) > 0.35);
        }
      }
    }
    live = select(total == 3, live && (total == 2 || total == 3), live);
    alive = select(0.0, 1.0, live);
  }
  let inner = select(0.0, 1.0, max(abs(fract(g) - 0.5)) < 0.45);
  let v = alive * inner * p_wet();
  return premul(vec3<f32>(v), select(1.0, v, p_transparent()));
}
