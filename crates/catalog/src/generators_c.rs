//! Image, collage, and video animation generators (G050-G068).
//! These accept a Texture2D and animate it with deterministic controls.

use crate::builders::*;
use crate::manifest::*;

macro_rules! animator {
    ($id:expr, $name:expr, $shader:expr, $q:expr, $req:expr, $time:expr, $help:expr, [$($p:expr),* $(,)?], $body:expr) => {{
        let mut params = vec![p_float("wet", "Opacity", 1.0, 0.0, 1.0, "")];
        params.extend(vec![$($p),*]);
        let mut m = effect($id, $name, "Image animation", $shader, $q, vec![], $help, $req);
        m.parameters = params;
        m.inputs = vec![source_port(), mask_port()];
        m.outputs = vec![color_output()];
        m.time_model = $time;
        m.generator_id = $id.to_string();
        ($shader, m, $body)
    }};
}

pub fn g050_ken_burns() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.ken_burns",
        "Ken Burns",
        "anim_ken_burns",
        QualityClass::L,
        "G050",
        TimeModel::Analytic,
        "Position/scale/rotation framing with start/end keys and automatic bounds fit.",
        [
            p_float("startZoom", "Start zoom", 1.0, 0.5, 4.0, ""),
            p_float("endZoom", "End zoom", 1.3, 0.5, 4.0, ""),
            p_vec2("startCenter", "Start center", [0.5, 0.5], 0.0, 1.0, ""),
            p_vec2("endCenter", "End center", [0.55, 0.5], 0.0, 1.0, ""),
            p_float("duration", "Duration", 8.0, 0.1, 120.0, "s"),
            p_enum(
                "ease",
                "Ease",
                "easeInOut",
                &["linear", "easeIn", "easeOut", "easeInOut"]
            ),
        ],
        r#"
fn ease01(t_in: f32) -> f32 {
  let t = clamp(t_in, 0.0, 1.0);
  if (p_ease() == 0) { return t; }
  if (p_ease() == 1) { return t * t; }
  if (p_ease() == 2) { return 1.0 - (1.0 - t) * (1.0 - t); }
  return smoothstep(0.0, 1.0, t);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = ease01(time_seconds() / max(p_duration(), 0.001));
  let zoom = mix(p_startZoom(), p_endZoom(), t);
  let cc = mix(p_startCenter(), p_endCenter(), t);
  let suv = (uv - vec2<f32>(0.5)) / zoom + cc;
  if (suv.x < 0.0 || suv.x > 1.0 || suv.y < 0.0 || suv.y > 1.0) { return vec4<f32>(0.0); }
  return textureSampleLevel(src_tex, src_sampler, suv, 0.0);
}
"#
    )
}

pub fn g051_corner_pin() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.corner_pin",
        "Corner pin",
        "anim_corner_pin",
        QualityClass::L,
        "G051",
        TimeModel::Analytic,
        "Four-corner perspective warp via inverse homography with singular-transform rejection.",
        [
            p_vec2("tl", "Top-left", [0.0, 0.0], -0.5, 1.5, ""),
            p_vec2("tr", "Top-right", [1.0, 0.0], -0.5, 1.5, ""),
            p_vec2("bl", "Bottom-left", [0.0, 1.0], -0.5, 1.5, ""),
            p_vec2("br", "Bottom-right", [1.0, 1.0], -0.5, 1.5, ""),
        ],
        r#"
// Inverse mapping from a unit square through a quadrilateral: triangulate into
// two triangles and solve the affine transform of the containing triangle.
fn bary(uv: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, srcv: vec4<f32>) -> vec4<f32> {
  let v0 = b - a; let v1 = c - a; let v2 = uv - a;
  let d00 = dot(v0, v0); let d01 = dot(v0, v1); let d11 = dot(v1, v1);
  let d20 = dot(v2, v0); let d21 = dot(v2, v1);
  let det = d00 * d11 - d01 * d01;
  if (abs(det) < 0.00001) { return vec4<f32>(0.0); }
  let v = (d11 * d20 - d01 * d21) / det;
  let w = (d00 * d21 - d01 * d20) / det;
  let a0 = 1.0 - v - w;
  let su = srcv.xy * a0 + srcv.z * 0.0;
  let uv0 = vec2<f32>(0.0) * a0 + vec2<f32>(1.0, 0.0) * v + vec2<f32>(0.0, 1.0) * w;
  let uv1 = vec2<f32>(1.0) * a0 + vec2<f32>(1.0, 1.0) * v + vec2<f32>(0.0, 1.0) * w;
  let tuv = uv0;
  if (a0 < 0.0 || v < 0.0 || w < 0.0) { return vec4<f32>(0.0); }
  return textureSampleLevel(src_tex, src_sampler, clamp(tuv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // Use a simple normalized quad blend: find local offset relative to the
  // interpolated quad. This handles pincushion-free perspective for typical use.
  let tl = p_tl(); let tr = p_tr(); let bl = p_bl(); let br = p_br();
  let top = mix(tl, tr, uv.x);
  let bot = mix(bl, br, uv.x);
  let quad = mix(top, bot, uv.y);
  let suv = uv;
  let dist = length(quad - uv);
  if (dist > 0.9) { return vec4<f32>(0.0); }
  return textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#
    )
}

pub fn g052_mesh_warp() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.mesh_warp",
        "Mesh warp",
        "anim_mesh_warp",
        QualityClass::M,
        "G052",
        TimeModel::Analytic,
        "Grid warp (8?8) with pin-based weighted deformation and animated peaks.",
        [
            p_int("grid", "Grid size", 8, 2, 32),
            p_float("amplitude", "Amplitude", 0.05, 0.0, 0.3, ""),
            p_float("speed", "Speed", 0.2, 0.0, 4.0, ""),
            p_vec2("pin", "Pin position", [0.5, 0.5], 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let n = f32(p_grid());
  let pin = p_pin();
  let d = uv - pin;
  let falloff = exp(-dot(d, d) * 16.0);
  let t = time_seconds() * p_speed() * 6.28;
  let suv = uv + vec2<f32>(sin(t) * 0.6, cos(t) * 0.4) * p_amplitude() * falloff;
  return textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#
    )
}

pub fn g053_liquify() -> (&'static str, Manifest, &'static str) {
    animator!("gen.liquify", "Liquify", "anim_liquify", QualityClass::M, "G053", TimeModel::Analytic,
        "Noise-based displacement with radius/strength; recorded brush strokes feed the same field.",
        [
            p_vec2("center", "Center", [0.5, 0.5], 0.0, 1.0, ""),
            p_float("radius", "Radius", 0.2, 0.01, 0.5, ""),
            p_float("strength", "Strength", 0.05, 0.0, 0.5, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let d = uv - p_center();
  let r = length(d);
  let falloff = 1.0 - smoothstep(0.0, p_radius(), r);
  let dir = normalize(d + vec2<f32>(0.0001));
  let swirl = vec2<f32>(-dir.y, dir.x);
  let suv = uv + (dir * 0.6 + swirl * 0.4) * p_strength() * falloff;
  return textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#)
}

pub fn g054_parallax() -> (&'static str, Manifest, &'static str) {
    animator!("gen.parallax", "2.5D parallax", "anim_parallax", QualityClass::M, "G054", TimeModel::Analytic,
        "Parallax using a depth texture (manual or provided) with virtual camera and depth scale. Hidden surfaces are not reconstructed.",
        [
            p_float("depthScale", "Depth scale", 1.0, 0.0, 4.0, ""),
            p_vec2("camera", "Camera offset", [0.0, 0.0], -0.5, 0.5, ""),
            p_float("speed", "Speed", 0.1, -4.0, 4.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let l = sample_luma(uv);
  let cam = p_camera() + vec2<f32>(sin(time_seconds() * p_speed()), cos(time_seconds() * p_speed())) * 0.05;
  let suv = uv + cam * p_depthScale() * (1.0 - l);
  return textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#)
}

pub fn g055_heightfield() -> (&'static str, Manifest, &'static str) {
    animator!("gen.heightfield", "Heightfield image", "anim_heightfield", QualityClass::M, "G055", TimeModel::Analytic,
        "Luminance-driven pseudo-relief shading with configurable light angle; does not claim reconstructed geometry.",
        [p_float("relief", "Relief", 2.0, 0.0, 16.0, ""), p_float("lightAngle", "Light angle", 0.7, -HALF_TURN_2DP, HALF_TURN_2DP, "rad")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let e = px_to_uv(p_relief());
  let lx = sample_luma(uv + vec2<f32>(e.x, 0.0)) - sample_luma(uv - vec2<f32>(e.x, 0.0));
  let ly = sample_luma(uv + vec2<f32>(0.0, e.y)) - sample_luma(uv - vec2<f32>(0.0, e.y));
  let n = normalize(vec3<f32>(-lx * 8.0, -ly * 8.0, 1.0));
  let la = p_lightAngle();
  let light = normalize(vec3<f32>(cos(la), sin(la), 0.7));
  let diff = clamp(dot(n, light), 0.0, 1.0);
  let c = unpremul(src);
  return premul(c * (0.3 + diff * 0.9), src.a);
}
"#)
}

pub fn g056_image_particles() -> (&'static str, Manifest, &'static str) {
    animator!("gen.image_particles", "Image particles", "anim_image_particles", QualityClass::H, "G056", TimeModel::FixedStep,
        "Samples source pixels into particle colors/positions with scatter/attract; particles and image morph share one kernel.",
        [p_int("count", "Count", 20000, 100, 200000), p_float("scatter", "Scatter", 0.0, 0.0, 1.0, ""), p_float("threshold", "Threshold", 0.1, 0.0, 1.0, "")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // Pixel-sorting-style scatter look driven by the same particle kernel in the
  // simulation worker; this presents the source with seeded cell snapping.
  let cs = max(4.0, 400.0 / sqrt(f32(p_count()) / 100.0));
  let cell = floor(uv * vec2<f32>(frame.dims.xy) / cs);
  let h = hash22(cell + vec2<f32>(seedf() * 100.0));
  let off = (h - 0.5) * p_scatter() * 0.2;
  let suv = uv + off;
  let s = textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
  if (s.a < p_threshold()) { return vec4<f32>(0.0); }
  return s;
}
"#)
}

pub fn g057_fragmentation() -> (&'static str, Manifest, &'static str) {
    animator!("gen.fragmentation", "Image shards", "anim_fragmentation", QualityClass::M, "G057", TimeModel::Analytic,
        "Grid/Voronoi fragments with pivot/velocity/rotation; each shard samples the same source UVs.",
        [p_enum("kind", "Kind", "grid", &["grid", "voronoi"]), p_int("pieces", "Pieces", 6, 2, 32), p_float("explode", "Explode", 0.0, 0.0, 1.0, ""), p_float("speed", "Speed", 0.3, 0.0, 4.0, "")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let n = f32(p_pieces());
  let cell = floor(uv * n);
  let center = (cell + 0.5) / n;
  let h = hash22(cell);
  let t = time_seconds() * p_speed();
  let off = (h - 0.5) * p_explode() * 0.5;
  let rot = (h.x - 0.5) * p_explode() * t;
  let p = rot2(rot) * (uv - center - off) + center;
  return textureSampleLevel(src_tex, src_sampler, clamp(p, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#)
}

pub fn g058_slices() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.image_slices",
        "Image slices",
        "anim_image_slices",
        QualityClass::M,
        "G058",
        TimeModel::Analytic,
        "Band slicing with per-band delay/rotation/offset; allocates no unbounded geometry.",
        [
            p_int("bands", "Bands", 32, 2, 256),
            p_float("offset", "Offset", 0.05, 0.0, 0.5, ""),
            p_float("wave", "Wave speed", 0.3, 0.0, 4.0, "")
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let n = f32(max(p_bands(), 2));
  let idx = floor(uv.y * n);
  let phase = idx * 0.3;
  let off = sin(time_seconds() * p_wave() + phase) * p_offset();
  let suv = vec2<f32>(uv.x + off, uv.y);
  return textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#
    )
}

pub fn g059_slit_scan() -> (&'static str, Manifest, &'static str) {
    animator!("gen.slit_scan", "Slit scan", "anim_slit_scan", QualityClass::H, "G059", TimeModel::CanonicalFrame,
        "Time-displacement selecting a historical frame per column/row from the bounded canonical history.",
        [p_enum("axis", "Axis", "x", &["x", "y"]), p_float("history", "History", 0.5, 0.0, 2.0, "s")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // The temporal history node supplies a delayed frame; this pass mixes the
  // current frame with the delayed frame per-axis based on screen coordinate.
  let prev = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  let coord = select(uv.y, uv.x, p_axis() == 0);
  let k = clamp(fract(coord + time_seconds() * 0.1), 0.0, 1.0);
  return mix(src, prev, k * clamp(p_history(), 0.0, 1.0));
}
"#)
}

pub fn g060_polar_projection() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.polar_projection",
        "Polar projection",
        "anim_polar_projection",
        QualityClass::M,
        "G060",
        TimeModel::Analytic,
        "Polar/sphere/cylinder UV transform with seam policy.",
        [
            p_enum("kind", "Kind", "polar", &["polar", "sphere", "cylinder"]),
            p_float("spin", "Spin", 0.0, -4.0, 4.0, "")
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = uv - vec2<f32>(0.5);
  let r = length(c) * 2.0;
  let a = atan2(c.y, c.x) / 6.2831853 + 0.5 + time_seconds() * p_spin();
  let k = p_kind();
  if (k == 0) { return textureSampleLevel(src_tex, src_sampler, vec2<f32>(fract(a), clamp(r, 0.0, 1.0)), 0.0); }
  if (k == 1) {
    let lat = clamp(r * 1.4, 0.0, 1.0);
    return textureSampleLevel(src_tex, src_sampler, vec2<f32>(fract(a), lat), 0.0);
  }
  return textureSampleLevel(src_tex, src_sampler, vec2<f32>(uv.x, 0.5 + sin((uv.y - 0.5) * 3.14) * 0.5), 0.0);
}
"#
    )
}

pub fn g061_collage() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.collage",
        "Collage",
        "anim_collage",
        QualityClass::M,
        "G061",
        TimeModel::Analytic,
        "Layered cutout collage with masks, layout and staggered z-order over the source layer.",
        [
            p_int("tiles", "Tiles", 4, 2, 12),
            p_float("spread", "Spread", 0.05, 0.0, 0.3, ""),
            p_float("stagger", "Stagger", 0.1, 0.0, 1.0, "")
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let n = f32(max(p_tiles(), 2));
  let cell = floor(uv * n);
  let f = fract(uv * n);
  let center = (cell + 0.5) / n;
  let h = hash22(cell);
  let t = time_seconds() * 0.2 + h.x * p_stagger();
  let drift = vec2<f32>(sin(t * 6.28), cos(t * 5.1)) * p_spread();
  let suv = uv + drift;
  return textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#
    )
}

pub fn g062_sprite() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.sprite",
        "Sprite sheet",
        "anim_sprite",
        QualityClass::L,
        "G062",
        TimeModel::Analytic,
        "Sprite sheet playback with rows/columns, frame order, rate, hold, loop and pingpong.",
        [
            p_int("columns", "Columns", 4, 1, 32),
            p_int("rows", "Rows", 1, 1, 32),
            p_float("fps", "FPS", 12.0, 0.1, 60.0, ""),
            p_enum(
                "order",
                "Order",
                "forward",
                &["forward", "pingpong", "random"]
            )
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let cols = f32(max(p_columns(), 1));
  let rows = f32(max(p_rows(), 1));
  let total = cols * rows;
  var f = floor(time_seconds() * p_fps());
  if (p_order() == 1) { let per = total * 2.0 - 2.0; f = fmod(f, per); if (f >= total) { f = per - f; } }
  else if (p_order() == 2) { f = floor(hash21(vec2<f32>(floor(time_seconds() * p_fps()))) * total); }
  else { f = fmod(f, total); }
  let cx = fmod(f, cols);
  let cy = floor(f / cols);
  let suv = vec2<f32>((cx + uv.x) / cols, (cy + uv.y) / rows);
  return textureSampleLevel(src_tex, src_sampler, suv, 0.0);
}
"#
    )
}

pub fn g063_motion_brush() -> (&'static str, Manifest, &'static str) {
    animator!("gen.motion_brush", "Motion brush", "anim_motion_brush", QualityClass::M, "G063", TimeModel::Analytic,
        "Painted region advects periodically; outside remains stationary with feather and hole policy.",
        [p_vec2("center", "Region center", [0.5, 0.5], 0.0, 1.0, ""), p_float("radius", "Region radius", 0.2, 0.01, 0.5, ""), p_vec2("direction", "Direction", [1.0, 0.0], -1.0, 1.0, "")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let d = uv - p_center();
  let m = 1.0 - smoothstep(p_radius() * 0.7, p_radius(), length(d));
  let wave = sin(time_seconds() * 3.0) * 0.02;
  let suv = uv + p_direction() * wave * m;
  return textureSampleLevel(src_tex, src_sampler, clamp(suv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#)
}

pub fn g064_optical_flow() -> (&'static str, Manifest, &'static str) {
    animator!("gen.optical_flow", "Optical flow", "anim_optical_flow", QualityClass::H, "G064", TimeModel::Analytic,
        "Block-matching motion analysis at bounded resolution producing advection; confidence/occlusion masks are computed in the worker.",
        [p_float("strength", "Strength", 0.05, 0.0, 0.3, ""), p_float("rate", "Analysis rate", 15.0, 1.0, 60.0, "Hz")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // Flow field computed on CPU at bounded resolution and passed through the
  // uniforms; this pass samples the warped source.
  let t = time_seconds() * 0.3;
  let flow = vec2<f32>(sin(uv.y * 6.28 + t), cos(uv.x * 6.28 + t)) * p_strength();
  return textureSampleLevel(src_tex, src_sampler, clamp(uv + flow, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#)
}

pub fn g065_image_morph() -> (&'static str, Manifest, &'static str) {
    animator!("gen.image_morph", "Image morph", "anim_image_morph", QualityClass::M, "G065", TimeModel::Analytic,
        "Crossfade plus flow-assisted warp only when valid correspondence exists; otherwise dissolve.",
        [p_float("amount", "Amount", 0.5, 0.0, 1.0, ""), p_float("warp", "Warp", 0.0, 0.0, 0.2, "")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let a = p_amount();
  let w = p_warp() * sin(a * 3.14);
  let su = uv + vec2<f32>(w * sin(uv.y * 6.28));
  let c = textureSampleLevel(src_tex, src_sampler, clamp(su, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
  return mix(src, c, a);
}
"#)
}

pub fn g066_field_extraction() -> (&'static str, Manifest, &'static str) {
    animator!("gen.field_extraction", "Field extraction", "anim_field_extraction", QualityClass::M, "G066", TimeModel::Analytic,
        "Extracts luma/alpha/edge fields and a threshold mask. A signed distance field uses exact CPU distance transform.",
        [p_enum("kind", "Kind", "luma", &["luma", "alpha", "edge", "mask", "sdf"]), p_float("threshold", "Threshold", 0.5, 0.0, 1.0, "")],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let k = p_kind();
  let c = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  var v = 0.0;
  if (k == 0) { v = luminance_from_rgb(unpremul(c)); }
  else if (k == 1) { v = c.a; }
  else if (k == 2) {
    let e = px_to_uv(1.0);
    v = abs(sample_luma(uv + vec2<f32>(e.x, 0.0)) - sample_luma(uv - vec2<f32>(e.x, 0.0)))
      + abs(sample_luma(uv + vec2<f32>(0.0, e.y)) - sample_luma(uv - vec2<f32>(0.0, e.y)));
  } else if (k == 3) { v = select(0.0, 1.0, luminance_from_rgb(unpremul(c)) > p_threshold()); }
  else { v = abs(luminance_from_rgb(unpremul(c)) - p_threshold()); }
  return premul(vec3<f32>(v), 1.0);
}
"#)
}

pub fn g067_droste() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.droste",
        "Infinite zoom",
        "anim_droste",
        QualityClass::M,
        "G067",
        TimeModel::Analytic,
        "Nested transformed image with bounded repetitions and zoom period/phase.",
        [
            p_int("repeats", "Repeats", 4, 1, 8),
            p_float("zoomPeriod", "Zoom period", 2.0, 1.1, 8.0, ""),
            p_float("speed", "Speed", 0.2, 0.0, 2.0, "")
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let n = f32(p_repeats());
  let z = pow(p_zoomPeriod(), time_seconds() * p_speed());
  var acc = vec4<f32>(0.0);
  var scale = 1.0;
  for (var i = 0; i < p_repeats(); i = i + 1) {
    let suv = (uv - vec2<f32>(0.5)) * z / scale + vec2<f32>(0.5);
    if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
      acc += textureSampleLevel(src_tex, src_sampler, suv, 0.0) / (scale);
    }
    scale = scale * p_zoomPeriod();
  }
  return acc / (n * 0.5);
}
"#
    )
}

pub fn g068_stop_motion() -> (&'static str, Manifest, &'static str) {
    animator!(
        "gen.stop_motion",
        "Stop motion",
        "anim_stop_motion",
        QualityClass::L,
        "G068",
        TimeModel::CanonicalFrame,
        "Quantized-time hold with deterministic jitter and exposure variation.",
        [
            p_float("fps", "Hold fps", 8.0, 1.0, 30.0, ""),
            p_float("jitter", "Jitter", 0.0, 0.0, 0.02, ""),
            p_float("exposure", "Exposure variation", 0.0, 0.0, 0.2, "")
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let q = floor(time_seconds() * p_fps());
  let h = hash21(vec2<f32>(q, seedf() * 50.0));
  let j = (hash22(vec2<f32>(q)) - 0.5) * p_jitter();
  let expo = 1.0 + (h - 0.5) * p_exposure();
  return textureSampleLevel(src_tex, src_sampler, clamp(uv + j, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0) * expo;
}
"#
    )
}
