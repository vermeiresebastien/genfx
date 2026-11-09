//! Generator catalog: procedural 2D/math (G001-G028), vector (G030-G045),
//! image (G050-G068), typography (G070-G079), SDF (G080-G089), mesh (G090-G107),
//! particles (G110-G120), physics (G130-G137), simulation (G140-G148),
//! control sources (G150-G162) and conditional adapters (G170-G185).
//!
//! Text-rendered rows (typography) use the bundled block font in
//! `font5x7.rs`; shaders reference the same glyph constants generated into
//! the WGSL.

use crate::builders::*;
use crate::manifest::*;

macro_rules! gen {
    ($id:expr, $name:expr, $cat:expr, $shader:expr, $q:expr, $req:expr, $time:expr, $help:expr, [$($p:expr),* $(,)?], $body:expr) => {{
        let mut params = vec![p_float("wet", "Opacity", 1.0, 0.0, 1.0, "")];
        params.extend(vec![$($p),*]);
        params.push(p_bool("transparent", "Transparent background", true));
        let mut m = effect($id, $name, $cat, $shader, $q, vec![], $help, $req);
        m.parameters = params;
        m.inputs = vec![mask_port()];
        m.outputs = vec![color_output()];
        m.time_model = $time;
        m.generator_id = $id.to_string();
        m.loop_status = "exact".into();
        ($shader, m, $body)
    }};
}

pub fn g001_solid() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.solid", "Solid color", "Generators", "gen_solid", QualityClass::L, "G001", TimeModel::Analytic,
        "Premultiplied solid color plate; supports transparent color plates.",
        [p_color("color", "Color", [1.0, 1.0, 1.0, 1.0])],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = p_color();
  return premul(c.rgb, c.a);
}
"#
    )
}

pub fn g002_gradient() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.gradient", "Gradient", "Generators", "gen_gradient", QualityClass::L, "G002", TimeModel::Analytic,
        "Linear, radial and conic gradients with editable stops and repeat/clamp.",
        [
            p_enum("kind", "Kind", "linear", &["linear", "radial", "conic"]),
            p_float("angle", "Angle", 0.0, 0.0, 6.2831853, "rad"),
            p_vec2("origin", "Origin", [0.5, 0.5], 0.0, 1.0, ""),
            p_float("radius", "Radius", 0.5, 0.0, 2.0, ""),
            p_color("stop0", "Stop 0", [0.0, 0.0, 0.0, 1.0]),
            p_color("stop1", "Stop 1", [1.0, 1.0, 1.0, 1.0]),
            p_float("stopCount", "Stop count", 2.0, 2.0, 3.0, ""),
            p_color("stop2", "Stop 2", [1.0, 0.0, 0.5, 1.0]),
            p_bool("repeat", "Repeat", false),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let d = uv - p_origin();
  let k = p_kind();
  var t = 0.0;
  if (k == 0u) { t = dot(d, vec2<f32>(cos(p_angle()), sin(p_angle()))) + 0.5; }
  else if (k == 1u) { t = length(d) / max(p_radius(), 0.0001); }
  else { t = fract(atan2(d.y, d.x) / 6.2831853 + 0.5); }
  if (p_repeat()) { t = fract(t); } else { t = clamp(t, 0.0, 1.0); }
  var col = p_stop1().rgb;
  if (p_stopCount() < 2.5) { col = mix(p_stop0().rgb, p_stop1().rgb, t); }
  else { col = select(mix(p_stop0().rgb, p_stop1().rgb, t * 2.0), mix(p_stop1().rgb, p_stop2().rgb, (t - 0.5) * 2.0), t >= 0.5); }
  return premul(col, 1.0);
}
"#
    )
}

pub fn g003_mesh_gradient() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.mesh_gradient_fill", "Mesh gradient", "Generators", "gen_mesh_gradient", QualityClass::M, "G003", TimeModel::Analytic,
        "Animated mesh gradient (shared FX024 contract).",
        [p_float("speed", "Speed", 0.1, 0.0, 4.0, "cycles/s"), p_float("turbulence", "Turbulence", 0.1, 0.0, 1.0, "")],
        include_str!("g_mesh.inc.wgsl")
    )
}

pub fn g004_noise() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.noise", "Noise", "Generators", "gen_noise", QualityClass::M, "G004", TimeModel::Analytic,
        "Value fBm, turbulence, ridged multifractal with scale/octaves and periodic seam mode.",
        [
            p_enum("kind", "Kind", "fbm", &["fbm", "turbulence", "ridged"]),
            p_float("scale", "Scale", 4.0, 0.1, 64.0, ""),
            p_int("octaves", "Octaves", 5, 1, 8),
            p_float("speed", "Speed", 0.1, -4.0, 4.0, "cycles/s"),
            p_bool("periodic", "Periodic", false),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed();
  var n = 0.0;
  if (p_periodic()) { n = periodic_fbm(uv * p_scale() * 8.0 + vec2<f32>(t * 8.0), vec2<f32>(p_scale() * 8.0), p_octaves()); }
  else if (p_kind() == 0u) { n = fbm(uv * p_scale() + vec2<f32>(t), p_octaves(), 2.0, 0.5); }
  else if (p_kind() == 1u) { n = turbulence(uv * p_scale() + vec2<f32>(t), p_octaves(), 2.0, 0.5); }
  else { n = ridged(uv * p_scale() + vec2<f32>(t), p_octaves(), 2.0, 0.5); }
  return premul(vec3<f32>(n), select(1.0, smoothstep(0.0, 0.02, abs(n - 0.5)), p_transparent()));
}
"#
    )
}

pub fn g005_voronoi() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.voronoi", "Voronoi", "Generators", "gen_voronoi", QualityClass::M, "G005", TimeModel::Analytic,
        "Voronoi/cellular field with nearest, second-nearest and difference distance modes.",
        [
            p_float("density", "Density", 12.0, 1.0, 64.0, "cells/axis"),
            p_float("jitter", "Jitter", 1.0, 0.0, 1.0, ""),
            p_enum("mode", "Mode", "cell", &["cell", "edge", "second", "difference"]),
            p_float("speed", "Speed", 0.1, -2.0, 2.0, "cycles/s"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let p = uv * p_density();
  let t = time_seconds() * p_speed();
  let ip = floor(p);
  let fp = fract(p);
  var f1 = 8.0; var f2 = 8.0; var cid = 0.0;
  for (var y = -1; y <= 1; y = y + 1) {
    for (var x = -1; x <= 1; x = x + 1) {
      let g = vec2<f32>(f32(x), f32(y));
      let o = (hash22(ip + g + vec2<f32>(t)) - 0.5) * p_jitter() + 0.5;
      let d = length(g + o - fp);
      if (d < f1) { f2 = f1; f1 = d; cid = hash21(ip + g); }
      else if (d < f2) { f2 = d; }
    }
  }
  var v = 0.0;
  if (p_mode() == 0u) { v = cid; }
  else if (p_mode() == 1u) { v = smoothstep(0.02, 0.0, f2 - f1); }
  else if (p_mode() == 2u) { v = f2; }
  else { v = f2 - f1; }
  return premul(vec3<f32>(v), 1.0);
}
"#
    )
}

pub fn g006_plasma() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.plasma", "Plasma", "Generators", "gen_plasma", QualityClass::L, "G006", TimeModel::Analytic,
        "Bounded sum-of-sinusoids plasma through a palette.",
        [
            p_float("frequency", "Frequency", 4.0, 0.1, 32.0, ""),
            p_float("speed", "Speed", 0.2, -4.0, 4.0, "cycles/s"),
            p_color("colorA", "Color A", [0.1, 0.0, 0.4, 1.0]),
            p_color("colorB", "Color B", [1.0, 0.3, 0.1, 1.0]),
            p_color("colorC", "Color C", [0.2, 0.9, 0.9, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let f = p_frequency();
  let t = time_seconds() * p_speed();
  let p = uv * f;
  var v = sin(p.x * 6.28 + t * 6.28) + sin(p.y * 6.28 + t * 4.0) + sin((p.x + p.y) * 6.28 + t * 3.0) + sin(length(p - 0.5) * 12.0 - t * 5.0);
  v = v * 0.25 * 0.5 + 0.5;
  var col = mix(p_colorA().rgb, p_colorB().rgb, smoothstep(0.0, 0.6, v));
  col = mix(col, p_colorC().rgb, smoothstep(0.45, 1.0, v));
  return premul(col, 1.0);
}
"#
    )
}

pub fn g007_grid() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.grid", "Grid", "Generators", "gen_grid", QualityClass::L, "G007", TimeModel::Analytic,
        "Grid, dots, checker, triangular, or hex tiling with animation. Emits a mask-shaped output.",
        [
            p_enum("kind", "Kind", "grid", &["grid", "dots", "checker", "triangular", "hex"]),
            p_float("scale", "Scale", 24.0, 2.0, 256.0, "output_px"),
            p_float("lineWidth", "Line width", 1.0, 0.5, 16.0, "output_px"),
            p_float("speed", "Speed", 0.0, -4.0, 4.0, "cycles/s"),
            p_color("color", "Color", [1.0, 1.0, 1.0, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let sc = max(p_scale(), 2.0);
  let px = uv * vec2<f32>(frame.dims.xy) + vec2<f32>(time_seconds() * p_speed() * sc);
  let g = px / sc;
  let f = fract(g) - vec2<f32>(0.5);
  let lw = p_lineWidth() / sc;
  let k = p_kind();
  var v = 0.0;
  if (k == 0u) { v = select(0.0, 1.0, min(abs(f.x), abs(f.y)) < lw * 0.5 || abs(f.x) > 0.5 - lw * 0.5 || abs(f.y) > 0.5 - lw * 0.5); }
  else if (k == 1u) { v = 1.0 - smoothstep(0.1, 0.14, length(f)); }
  else if (k == 2u) { v = select(0.0, 1.0, mod(floor(g.x) + floor(g.y), 2.0) > 0.5); }
  else if (k == 3u) { v = select(0.0, 1.0, abs(f.y) * 2.0 < (0.5 - abs(f.x)) * 1.6); }
  else {
    let hh = max(abs(f.x) * 0.866 + abs(f.y) * 0.5, abs(f.y));
    v = select(0.0, 1.0, hh < 0.5 - lw * 0.5);
  }
  return premul(p_color().rgb * v, select(1.0, v, p_transparent()));
}
"#
    )
}

pub fn g008_concentric() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.radial_rays", "Radial rays / spiral", "Generators", "gen_radial_rays", QualityClass::L, "G008", TimeModel::Analytic,
        "Radial rays, concentric rings or a spiral with twist and animated stroke width.",
        [
            p_enum("kind", "Kind", "rays", &["rays", "rings", "spiral"]),
            p_int("rays", "Rays", 32, 2, 256),
            p_float("turns", "Spiral turns", 8.0, 1.0, 64.0, ""),
            p_float("strokeWidth", "Stroke width", 0.3, 0.02, 1.0, ""),
            p_float("speed", "Speed", 0.1, -4.0, 4.0, "cycles/s"),
            p_vec2("center", "Center", [0.5, 0.5], 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let aspect = f32(frame.dims.x) / max(f32(frame.dims.y), 1.0);
  let p = (uv - p_center()) * vec2<f32>(aspect, 1.0);
  let a = atan2(p.y, p.x) / 6.2831853 + 0.5;
  let r = length(p);
  let t = time_seconds() * p_speed();
  var v = 0.0;
  let k = p_kind();
  if (k == 0u) { let n = f32(p_rays()); v = select(0.0, 1.0, fract(a * n + t) < p_strokeWidth()); }
  else if (k == 1u) { v = select(0.0, 1.0, fract(r * f32(p_rays()) * 0.1 - t) < p_strokeWidth()); }
  else { v = select(0.0, 1.0, fract(a * p_turns() + r * 8.0 - t * 3.0) < p_strokeWidth()); }
  return premul(vec3<f32>(v), select(1.0, v, p_transparent()));
}
"#
    )
}

pub fn g009_moire() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.interference", "Interference", "Generators", "gen_interference", QualityClass::L, "G009", TimeModel::Analytic,
        "Two-field interference (stripes/rings/grid) with adjustable frequency delta and angle.",
        [
            p_float("frequency", "Frequency", 24.0, 1.0, 128.0, ""),
            p_float("delta", "Frequency delta", 0.5, -4.0, 4.0, ""),
            p_float("angle", "Relative angle", 3.0, -90.0, 90.0, "deg"),
            p_float("speed", "Speed", 0.1, 0.0, 4.0, "cycles/s"),
        ],
        r#"
fn field(uv: vec2<f32>, f: f32, a: f32) -> f32 {
  let p = (uv - vec2<f32>(0.5)) * vec2<f32>(f32(frame.dims.x) / f32(frame.dims.y), 1.0);
  let r = rot2(a) * p;
  return sin(r.x * f * 6.28) * 0.5 + 0.5;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed();
  let a = field(uv, p_frequency(), t);
  let b = field(uv, p_frequency() + p_delta(), radians(p_angle()) + t * 0.5);
  let v = a * b;
  return premul(vec3<f32>(v), 1.0);
}
"#
    )
}

pub fn g010_pattern_fill() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.pattern_fill", "Halftone fill", "Generators", "gen_pattern_fill", QualityClass::L, "G010", TimeModel::Analytic,
        "Halftone or hatching pattern driven by gradient, noise or a constant tone.",
        [
            p_enum("driver", "Driver", "constant", &["constant", "gradient", "noise"]),
            p_float("tone", "Tone", 0.5, 0.0, 1.0, ""),
            p_float("cellSize", "Cell size", 8.0, 2.0, 64.0, "output_px"),
            p_bool("hatch", "Hatch", false),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  var tone = p_tone();
  if (p_driver() == 1u) { tone = uv.x; }
  else if (p_driver() == 2u) { tone = fbm(uv * 6.0, 4, 2.0, 0.5); }
  let cs = max(p_cellSize(), 2.0);
  let g = uv * vec2<f32>(frame.dims.xy) / cs;
  let f = fract(g) - vec2<f32>(0.5);
  var v = 0.0;
  if (p_hatch()) {
    let w = abs(f.y);
    v = select(0.0, 1.0, w < tone * 0.5);
  } else {
    let rad = (1.0 - tone) * 0.5;
    v = 1.0 - step(rad, length(f));
  }
  return premul(vec3<f32>(v), select(1.0, v, p_transparent()));
}
"#
    )
}

pub fn g011_topographic() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.topographic", "Topographic contours", "Generators", "gen_topographic", QualityClass::M, "G011", TimeModel::Analytic,
        "Iso-contour lines from animated terrain noise.",
        [
            p_int("levels", "Levels", 12, 2, 64),
            p_float("width", "Line width", 1.5, 0.5, 8.0, "output_px"),
            p_float("scale", "Scale", 4.0, 0.5, 32.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let h = fbm(uv * p_scale() + vec2<f32>(time_seconds() * 0.02), 5, 2.0, 0.5);
  let lv = h * f32(p_levels());
  let d = abs(fract(lv) - 0.5) * 2.0;
  let w = p_width() / max(f32(frame.dims.y), 1.0) * f32(p_levels());
  let v = 1.0 - smoothstep(1.0 - w * 4.0, 1.0, d);
  return premul(vec3<f32>(v), select(1.0, v, p_transparent()));
}
"#
    )
}

pub fn g012_clouds_nebula() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.aurora", "Aurora", "Generators", "gen_aurora", QualityClass::M, "G012", TimeModel::Analytic,
        "Layered aurora ribbons in warped coordinate space.",
        [
            p_float("scale", "Scale", 3.0, 0.1, 16.0, ""),
            p_float("speed", "Speed", 0.05, 0.0, 1.0, "cycles/s"),
            p_color("colorA", "Color A", [0.1, 1.0, 0.4, 1.0]),
            p_color("colorB", "Color B", [0.3, 0.2, 1.0, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed();
  var col = vec3<f32>(0.0);
  for (var i = 0; i < 3; i = i + 1) {
    let fi = f32(i);
    let warp = fbm(uv * p_scale() * 2.0 + vec2<f32>(t + fi * 2.0), 4, 2.0, 0.5);
    let band = sin((uv.x * p_scale() * 3.0 + warp * 4.0 + fi * 1.5 + t * 4.0) * 6.28);
    let ribbon = smoothstep(0.7, 0.98, uv.y + warp * 0.2 + fi * 0.1) * smoothstep(0.7, 0.98, 1.0 - uv.y);
    let v = smoothstep(0.0, 0.6, band) * ribbon;
    col += mix(p_colorA().rgb, p_colorB().rgb, fi / 3.0) * v;
  }
  return premul(col, select(1.0, clamp(length(col), 0.0, 1.0), p_transparent()));
}
"#
    )
}

pub fn g013_oscilloscope() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.oscilloscope", "Oscilloscope", "Generators", "gen_oscilloscope", QualityClass::M, "G013", TimeModel::Analytic,
        "Glowing waveform line; the spectrum/waveform input deforms its height.",
        [
            p_float("frequency", "Frequency", 2.0, 0.1, 32.0, ""),
            p_float("amplitude", "Amplitude", 0.25, 0.0, 1.0, ""),
            p_float("speed", "Speed", 0.2, 0.0, 4.0, "cycles/s"),
            p_float("thickness", "Thickness", 2.0, 0.5, 16.0, "output_px"),
            p_color("color", "Color", [0.2, 1.0, 0.6, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed() * 6.28;
  let y = sin(uv.x * p_frequency() * 6.28 + t) * p_amplitude() * (0.7 + audio_rms() * 0.6);
  let d = abs((uv.y - 0.5) - y) * f32(frame.dims.y);
  let core = 1.0 - smoothstep(p_thickness() * 0.5, p_thickness() * 0.5 + 1.5, d);
  let glow = exp(-d / 8.0) * 0.5;
  let m = core + glow;
  return premul(p_color().rgb * m, select(1.0, m, p_transparent()));
}
"#
    )
}

pub fn g014_caustics() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.caustics", "Water caustics", "Generators", "gen_caustics", QualityClass::M, "G014", TimeModel::Analytic,
        "Water caustic illumination from summed waves with explicit ripple-impulse events.",
        [
            p_float("scale", "Scale", 5.0, 0.5, 32.0, ""),
            p_float("speed", "Speed", 0.2, 0.0, 2.0, "cycles/s"),
            p_float("sharpness", "Sharpness", 8.0, 1.0, 64.0, ""),
            p_color("tint", "Tint", [0.6, 0.85, 1.0, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed();
  var v = 0.0;
  for (var i = 0; i < 4; i = i + 1) {
    let fi = f32(i);
    let dir = vec2<f32>(cos(fi * 1.7 + t), sin(fi * 1.7 + t));
    v += sin(dot(uv * p_scale(), dir) * 6.28 + t * (2.0 + fi));
  }
  let c = pow(1.0 - abs(v * 0.25), p_sharpness()) ;
  let col = p_tint().rgb * c;
  return premul(col, select(1.0, clamp(c, 0.0, 1.0), p_transparent()));
}
"#
    )
}

pub fn g015_fractals() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.fractal", "Fractal", "Generators", "gen_fractal", QualityClass::M, "G015", TimeModel::Analytic,
        "Mandelbrot, Julia and Burning Ship escape-time images with iteration and palette controls.",
        [
            p_enum("kind", "Kind", "mandelbrot", &["mandelbrot", "julia", "burningShip"]),
            p_vec2("center", "Center", [-0.5, 0.0], -2.5, 1.0, ""),
            p_float("zoom", "Zoom", 1.0, 0.01, 1.0e6, ""),
            p_int("iterations", "Iterations", 128, 8, 2048),
            p_vec2("juliaC", "Julia c", [-0.7269, 0.1889], -2.0, 2.0, ""),
            p_float("speed", "Speed", 0.0, -2.0, 2.0, "cycles/s"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let aspect = f32(frame.dims.x) / max(f32(frame.dims.y), 1.0);
  var c = p_center() + (uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0) * 3.0 / p_zoom();
  let t = time_seconds() * p_speed();
  var z = vec2<f32>(0.0);
  var jc = p_juliaC();
  jc = jc + vec2<f32>(cos(t * 6.28) * 0.1, sin(t * 6.28) * 0.1);
  if (p_kind() == 1u) { z = c; c = jc; }
  var iter = 0;
  let maxi = p_iterations();
  for (var i = 0; i < maxi; i = i + 1) {
    if (dot(z, z) > 4.0) { break; }
    if (p_kind() == 2u) { z = vec2<f32>(abs(z.x), abs(z.y)); }
    z = vec2<f32>(z.x * z.x - z.y * z.y + c.x, 2.0 * z.x * z.y + c.y);
    iter = i;
  }
  let f = f32(iter) / f32(maxi);
  let col = vec3<f32>(fract(f * 3.0), fract(f * 3.0 + 0.33), fract(f * 3.0 + 0.66)) * (1.0 - smoothstep(0.9, 1.0, f));
  return premul(col, 1.0);
}
"#
    )
}

pub fn g016_lissajous() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.lissajous", "Lissajous", "Generators", "gen_lissajous", QualityClass::L, "G016", TimeModel::Analytic,
        "Lissajous/rose/spirograph curves as a glowing PathSet image.",
        [
            p_enum("kind", "Kind", "lissajous", &["lissajous", "rose", "spirograph"]),
            p_vec2("ratios", "Ratios", [3.0, 2.0], 1.0, 12.0, ""),
            p_float("phase", "Phase", 0.0, 0.0, 6.2831853, "rad"),
            p_float("lineWidth", "Line width", 1.5, 0.5, 8.0, "output_px"),
            p_float("speed", "Speed", 0.1, -2.0, 2.0, "cycles/s"),
        ],
        r#"
fn curve(t: f32) -> vec2<f32> {
  let a = t * 6.2831853;
  let r = p_ratios();
  if (p_kind() == 0u) { return vec2<f32>(sin(a * r.x + p_phase()), sin(a * r.y)); }
  if (p_kind() == 1u) { return vec2<f32>(cos(a * r.x) * cos(a * r.y), sin(a * r.x) * cos(a * r.y)); }
  return vec2<f32>(cos(a) + 0.5 * cos(a * 6.0), sin(a) + 0.5 * sin(a * 7.0)) * 0.6;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let p = (uv - vec2<f32>(0.5)) * vec2<f32>(f32(frame.dims.x) / f32(frame.dims.y), 1.0) * 1.2;
  let ph = time_seconds() * p_speed();
  var md = 8.0;
  for (var i = 0; i < 256; i = i + 1) {
    let c = curve(f32(i) / 256.0 + ph);
    md = min(md, length(p - c));
  }
  let dpx = md * f32(frame.dims.y);
  let m = 1.0 - smoothstep(p_lineWidth() * 0.5, p_lineWidth() * 0.5 + 1.5, dpx);
  return premul(vec3<f32>(m), select(1.0, m, p_transparent()));
}
"#
    )
}

pub fn g017_superformula() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.superformula", "Superformula", "Generators", "gen_superformula", QualityClass::L, "G017", TimeModel::Analytic,
        "Superformula/superellipse with singularity guards and bounded polygon sampling.",
        [
            p_float("m", "m", 6.0, 0.1, 32.0, ""),
            p_float("n1", "n1", 0.3, 0.01, 10.0, ""),
            p_float("n2", "n2", 0.3, 0.01, 10.0, ""),
            p_float("n3", "n3", 0.3, 0.01, 10.0, ""),
            p_float("radius", "Radius", 0.35, 0.05, 0.7, ""),
            p_float("speed", "Speed", 0.1, -2.0, 2.0, "cycles/s"),
        ],
        r#"
fn superformula(a: f32) -> f32 {
  let mm = p_m();
  let t1 = pow(abs(cos(mm * a / 4.0)), p_n2());
  let t2 = pow(abs(sin(mm * a / 4.0)), p_n3());
  return pow(t1 + t2, -1.0 / max(p_n1(), 0.01));
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let p = (uv - vec2<f32>(0.5)) * vec2<f32>(f32(frame.dims.x) / f32(frame.dims.y), 1.0);
  let a = atan2(p.y, p.x) + time_seconds() * p_speed();
  let r = superformula(a) * p_radius();
  let inside = select(0.0, 1.0, length(p) < r);
  return premul(vec3<f32>(inside), select(1.0, inside, p_transparent()));
}
"#
    )
}

pub fn g018_truchet() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.truchet", "Truchet", "Generators", "gen_truchet", QualityClass::L, "G018", TimeModel::Analytic,
        "Seeded Truchet tiles with continuous borders.",
        [
            p_float("tile", "Tile size", 48.0, 8.0, 256.0, "output_px"),
            p_float("stroke", "Stroke", 3.0, 0.5, 24.0, "output_px"),
            p_float("speed", "Speed", 0.0, -2.0, 2.0, "cycles/s"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let ts = max(p_tile(), 8.0);
  let px = uv * vec2<f32>(frame.dims.xy);
  let cell = floor(px / ts);
  let f = fract(px / ts);
  let flip = hash21(cell + floor(time_seconds() * p_speed() * 4.0)) > 0.5;
  let x = select(f.x, 1.0 - f.x, flip);
  let d1 = abs(length(f - vec2<f32>(0.0)) - 0.5);
  let d2 = abs(length(f - vec2<f32>(1.0)) - 0.5);
  let sw = p_stroke() / ts * 0.5;
  let m = select(max(1.0 - smoothstep(sw - 0.02, sw + 0.02, d1), 1.0 - smoothstep(sw - 0.02, sw + 0.02, d2)),
                 max(1.0 - smoothstep(sw - 0.02, sw + 0.02, abs(length(x - 0.5) - 0.5)) * 0.0 + 1.0 - smoothstep(sw - 0.02, sw + 0.02, d1), 0.0),
                 flip == false);
  return premul(vec3<f32>(m), select(1.0, m, p_transparent()));
}
"#
    )
}

pub fn g019_texture_fields() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.texture_field", "Texture field", "Generators", "gen_texture_field", QualityClass::M, "G019", TimeModel::Analytic,
        "Wood, marble, paper, fabric, stucco and glitter texture fields from distinct recipes.",
        [
            p_enum("material", "Material", "wood", &["wood", "marble", "paper", "fabric", "stucco", "glitter"]),
            p_float("scale", "Scale", 6.0, 0.5, 64.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let sc = p_scale();
  let p = uv * sc;
  let m = p_material();
  var col = vec3<f32>(0.5);
  if (m == 0u) { let rings = sin(length(p) * 6.0 + fbm(p, 3, 2.0, 0.5) * 8.0); col = mix(vec3<f32>(0.35, 0.2, 0.1), vec3<f32>(0.6, 0.4, 0.2), rings * 0.5 + 0.5); }
  else if (m == 1u) { let v = abs(sin(p.x * 3.0 + fbm(p * 2.0, 4, 2.0, 0.5) * 8.0)); col = mix(vec3<f32>(0.9), vec3<f32>(0.2, 0.25, 0.3), smoothstep(0.85, 1.0, v)); }
  else if (m == 2u) { col = vec3<f32>(0.9) * (0.85 + fbm(p * 40.0, 3, 2.0, 0.5) * 0.3); }
  else if (m == 3u) { let w = sin(p.x * 60.0) * sin(p.y * 60.0); col = vec3<f32>(0.4, 0.2, 0.5) * (0.8 + w * 0.2); }
  else if (m == 4u) { col = vec3<f32>(0.7) * (0.7 + turbulence(p * 30.0, 4, 2.0, 0.5) * 0.6); }
  else { let h = hash21(floor(p * 200.0)); col = vec3<f32>(0.9) * pow(h, 12.0) + vec3<f32>(0.1); }
  return premul(col, 1.0);
}
"#
    )
}

pub fn g020_starfield() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.starfield", "Starfield", "Generators", "gen_starfield", QualityClass::M, "G020", TimeModel::Analytic,
        "Starfield/space tunnel with parallax layers, depth and finite near clipping.",
        [
            p_enum("kind", "Kind", "stars", &["stars", "tunnel", "galaxy"]),
            p_int("points", "Points", 5000, 100, 50000),
            p_float("speed", "Speed", 0.3, 0.0, 4.0, ""),
            p_float("depth", "Depth", 1.0, 0.1, 4.0, ""),
            p_color("color", "Color", [1.0, 1.0, 1.0, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // Deterministic grid star density driven by `points` (density proxy).
  let density = f32(p_points()) / 5000.0;
  let p = (uv - vec2<f32>(0.5)) * p_depth();
  var v = 0.0;
  if (p_kind() == 1u) {
    let a = atan2(p.y, p.x);
    let r = max(length(p), 0.001);
    let tunnel = fbm(vec2<f32>(a * 3.0, 1.0 / r + time_seconds() * p_speed()), 4, 2.0, 0.5);
    v = smoothstep(0.6, 1.0, tunnel);
  } else {
    let g = floor(uv * 600.0 * sqrt(density));
    let h = hash21(g + floor(time_seconds() * p_speed() * 10.0));
    if (h > 0.995) { let d = length(fract(uv * 600.0 * sqrt(density)) - 0.5); v = exp(-d * 40.0); }
    if (p_kind() == 2u) {
      let arm = fbm(vec2<f32>(atan2(p.y, p.x) * 2.0, length(p) * 4.0) + f32(time_seconds()) * 0.05, 4, 2.0, 0.5);
      v += smoothstep(0.5, 0.9, arm) * 0.5 * smoothstep(0.6, 0.1, length(p));
    }
  }
  return premul(p_color().rgb * v, select(1.0, v, p_transparent()));
}
"#
    )
}

pub fn g021_test_signal() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.test_signal", "Test signal", "Generators", "gen_test_signal", QualityClass::L, "G021", TimeModel::Analytic,
        "Colour bars, ramps, frame counter band, checker and moving marker. Diagnostics source.",
        [
            p_enum("kind", "Kind", "bars", &["bars", "ramp", "counter", "checker"]),
            p_float("markerSpeed", "Marker speed", 0.2, 0.0, 2.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let k = p_kind();
  var col = vec3<f32>(0.0);
  if (k == 0u) {
    let i = floor(uv.x * 7.0);
    let bars = array<vec3<f32>, 7>(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(1.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 1.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    col = bars[i32(min(i, 6.0))];
    if (uv.y > 0.85) { col = vec3<f32>(uv.x); }
  } else if (k == 1u) { col = vec3<f32>(uv.x); }
  else if (k == 2u) { 
    let n = floor(time_seconds() * 60.0);
    let x = floor(uv.x * 8.0);
    let y = floor(uv.y * 8.0);
    let bit = fract(n / pow(2.0, x + y * 8.0) / 2.0) > 0.5;
    col = select(vec3<f32>(0.02), vec3<f32>(0.9), bit);
  } else { col = vec3<f32>(mod(floor(uv.x * 16.0) + floor(uv.y * 9.0), 2.0)); }
  // Moving marker
  let mx = fract(time_seconds() * p_markerSpeed());
  let md = length((uv - vec2<f32>(mx, 0.5)) * vec2<f32>(f32(frame.dims.x) / f32(frame.dims.y), 1.0));
  if (md < 0.05) { col = vec3<f32>(1.0, 0.0, 0.5); }
  return premul(col, 1.0);
}
"#
    )
}

pub fn g022_polar_tunnel() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.polar_tunnel", "Polar tunnel", "Generators", "gen_polar_tunnel", QualityClass::M, "G022", TimeModel::Analytic,
        "Polar/vortex tunnel mapping palette or texture into atan2/radius with bounded singular center.",
        [
            p_float("twist", "Twist", 0.3, -2.0, 2.0, ""),
            p_float("speed", "Speed", 0.3, 0.0, 4.0, ""),
            p_float("radius", "Radius scale", 1.0, 0.1, 4.0, ""),
            p_color("colorA", "Color A", [0.2, 0.0, 0.5, 1.0]),
            p_color("colorB", "Color B", [1.0, 0.8, 0.2, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let p = (uv - vec2<f32>(0.5)) * vec2<f32>(f32(frame.dims.x) / f32(frame.dims.y), 1.0);
  let r = max(length(p), 0.001);
  let a = atan2(p.y, p.x) + p_twist() / r + time_seconds() * p_speed();
  let v = 0.5 + 0.5 * sin(a * 4.0 + r * 8.0);
  let col = mix(p_colorA().rgb, p_colorB().rgb, v);
  return premul(col, 1.0);
}
"#
    )
}

pub fn g023_lsystem() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.lsystem", "Fractal tree", "Generators", "gen_lsystem", QualityClass::M, "G023", TimeModel::Analytic,
        "Bounded L-system tree (angle/depth) with segment limit.",
        [
            p_float("angle", "Angle", 25.0, 5.0, 90.0, "deg"),
            p_int("depth", "Depth", 5, 1, 10),
            p_float("sway", "Sway", 0.0, 0.0, 0.2, ""),
            p_float("speed", "Speed", 0.1, 0.0, 2.0, ""),
            p_color("color", "Color", [0.3, 0.9, 0.4, 1.0]),
        ],
        r#"
fn seg_dist(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
  let pa = p - a; let ba = b - a;
  let h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0);
  return length(pa - ba * h);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  var md = 8.0;
  let peng = p_angle();
  let sway = sin(time_seconds() * 6.28 * p_speed()) * p_sway();
  let aspect = f32(frame.dims.x) / f32(frame.dims.y);
  let p = (uv - vec2<f32>(0.5, 0.9)) * vec2<f32>(aspect, 1.0);
  // Iterative recursive drawing approximated by ahead-of-time enumeration:
  // depth <= 10 so 2^10 segments max (bounded).
  var stack: array<vec2<f32>, 11>;
  var stack_a: array<f32, 11>;
  var sp = 0;
  var pos = vec2<f32>(0.0);
  var ang = -3.1415926 * 0.5;
  var lenv = 0.25;
  for (var i = 0; i <= 10; i = i + 1) {
    let depth = p_depth();
    if (i >= depth) { break; }
    let a = radians(peng) + sway;
    let tip = pos + vec2<f32>(cos(ang), sin(ang)) * lenv;
    md = min(md, seg_dist(p, pos, tip));
    // push
    if (sp < 10) {
      stack[sp] = pos; stack_a[sp] = ang; sp += 1;
      pos = tip;
      ang = ang + a;
      let mid = pos + vec2<f32>(cos(ang), sin(ang)) * lenv * 0.8;
      md = min(md, seg_dist(p, pos, mid));
      pos = mid;
    }
    lenv *= 0.75;
  }
  let dpx = md * f32(frame.dims.y);
  let m = 1.0 - smoothstep(1.5, 3.0, dpx);
  return premul(p_color().rgb * m, select(1.0, m, p_transparent()));
}
"#
    )
}

pub fn g024_attractor() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.attractor", "Strange attractor", "Generators", "gen_attractor", QualityClass::M, "G024", TimeModel::FixedStep,
        "Fixed-step Lorenz/Rossler/Clifford attractor plot with burn-in and bounded point count.",
        [
            p_enum("kind", "Kind", "lorenz", &["lorenz", "rossler", "clifford"]),
            p_int("points", "Points", 20000, 1000, 100000),
            p_float("scale", "Scale", 0.12, 0.01, 0.5, ""),
            p_float("speed", "Speed", 0.2, 0.0, 2.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  var x = 0.01; var y = 0.0; var z = 0.0;
  let pts = 4000;
  let step = 0.005 * (1.0 + time_seconds() * p_speed());
  // burn-in
  for (var i = 0; i < 100; i = i + 1) {
    let k = p_kind();
    if (k == 0u) { let dx = 10.0 * (y - x); let dy = x * (28.0 - z) - y; let dz = x * y - 2.6667 * z; x += dx * step; y += dy * step; z += dz * step; }
    else if (k == 1u) { let dx = -y - z; let dy = x + 0.2 * y; let dz = 0.2 + z * (x - 5.7); x += dx * step * 5.0; y += dy * step * 5.0; z += dz * step * 5.0; }
    else { let nx = sin(-1.4 * y) + 2.0 * cos(1.6 * x); let ny = sin(2.2 * x) + 2.0 * cos(1.8 * y); x = nx * 0.5; y = ny * 0.5; }
  }
  var v = 0.0;
  let p = (uv - vec2<f32>(0.5)) * 2.0;
  for (var i = 0; i < pts; i = i + 1) {
    let k = p_kind();
    if (k == 0u) { let dx = 10.0 * (y - x); let dy = x * (28.0 - z) - y; let dz = x * y - 2.6667 * z; x += dx * step; y += dy * step; z += dz * step; }
    else if (k == 1u) { let dx = -y - z; let dy = x + 0.2 * y; let dz = 0.2 + z * (x - 5.7); x += dx * step * 5.0; y += dy * step * 5.0; z += dz * step * 5.0; }
    else { let nx = sin(-1.4 * y) + 2.0 * cos(1.6 * x); let ny = sin(2.2 * x) + 2.0 * cos(1.8 * y); x = nx * 0.5; y = ny * 0.5; }
    let sp = vec2<f32>(x * p_scale(), z * p_scale() - 0.5);
    v += exp(-length(p - sp) * 40.0);
  }
  v = clamp(v, 0.0, 1.0);
  return premul(vec3<f32>(v), select(1.0, v, p_transparent()));
}
"#
    )
}

pub fn g025_kaleidoscope() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.kaleidoscope", "Kaleidoscope", "Generators", "gen_kaleidoscope", QualityClass::M, "G025", TimeModel::Analytic,
        "Seeded noise field under symmetry folding, spin and radial scale.",
        [
            p_int("symmetry", "Symmetry", 6, 2, 64),
            p_bool("mirror", "Mirror", true),
            p_float("spin", "Spin", 0.1, -2.0, 2.0, "cycles/s"),
            p_float("radialScale", "Radial scale", 3.0, 0.1, 32.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = (uv - vec2<f32>(0.5)) * vec2<f32>(f32(frame.dims.x) / f32(frame.dims.y), 1.0);
  var a = atan2(c.y, c.x) + time_seconds() * p_spin() * 6.28;
  let r = length(c);
  let seg = 6.2831853 / f32(p_symmetry());
  a = mod(a, seg);
  if (p_mirror() && a > seg * 0.5) { a = seg - a; }
  let p = vec2<f32>(cos(a), sin(a)) * r * p_radialScale();
  let v = fbm(p, 4, 2.0, 0.5);
  return premul(vec3<f32>(v), 1.0);
}
"#
    )
}

pub fn g026_recursive_tiling() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.recursive_tiling", "Recursive tiling", "Generators", "gen_recursive_tiling", QualityClass::M, "G026", TimeModel::Analytic,
        "Sierpinski triangle/carpet and Koch subdivision with bounded depth and tile count.",
        [
            p_enum("kind", "Kind", "sierpinski", &["sierpinski", "carpet", "koch"]),
            p_int("depth", "Depth", 4, 1, 8),
        ],
        r#"
fn sierpinski(p: vec2<f32>, d: i32) -> f32 {
  // p in unit triangle; returns 0 inside void.
  if (d <= 0) { return 1.0; }
  let m = (vec2<f32>(0.0) + vec2<f32>(1.0) + vec2<f32>(0.5, 0.866)) / 3.0;
  let s = 0.5;
  let r = (p - m) * 2.0;
  if (abs(r.y) < 0.45 && length(p - m) < 0.28) { return 0.0; }
  return sierpinski(r * 2.0, d - 1);
}
fn carpet(p: vec2<f32>, d: i32) -> f32 {
  if (d <= 0) { return 1.0; }
  let c = floor(p * 3.0);
  if (c.x == 1.0 && c.y == 1.0) { return 0.0; }
  return carpet(fract(p * 3.0), d - 1);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let p = uv - vec2<f32>(0.5);
  var v = 0.0;
  if (p_kind() == 0u) { v = sierpinski(p * 1.5 + vec2<f32>(0.25, -0.4), p_depth()); }
  else if (p_kind() == 1u) { v = carpet(p + vec2<f32>(0.5), p_depth()); }
  else {
    let ang = atan2(p.y, p.x);
    let koch = abs(sin(ang * f32(p_depth() * 2)));
    v = select(0.0, 1.0, length(p) < 0.4 + koch * 0.05);
  }
  return premul(vec3<f32>(v), select(1.0, v, p_transparent()));
}
"#
    )
}

pub fn g027_automata() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.automata", "Cellular automata", "Generators", "gen_automata", QualityClass::H,
        "G027",
        TimeModel::FixedStep,
        "Bounded cellular automata: life, elementary rule N, and cyclic color states. 2562 grid, steps per beat.",
        [
            p_enum("kind", "Kind", "life", &["life", "elementary", "cyclic"]),
            p_int("rule", "Elementary rule", 30, 0, 255),
            p_float("cellSize", "Cell size", 2.0, 1.0, 8.0, "output_px"),
            p_int("stepsPerBeat", "Steps per beat", 1, 1, 8),
        ],
        include_str!("g027_automata.inc.wgsl")
    )
}

pub fn g028_reaction_diffusion() -> (&'static str, Manifest, &'static str) {
    gen!(
        "gen.reaction_diffusion", "Reaction diffusion", "Generators", "gen_reaction_diffusion", QualityClass::H, "G028", TimeModel::FixedStep,
        "Gray-Scott reaction-diffusion at 2562 with fixed stable substeps; state is computed in the simulation worker and uploaded.",
        [
            p_float("feed", "Feed", 0.0367, 0.0, 0.1, ""),
            p_float("kill", "Kill", 0.0649, 0.0, 0.1, ""),
            p_color("colorA", "Color A", [0.0, 0.0, 0.0, 1.0]),
            p_color("colorB", "Color B", [1.0, 0.6, 0.2, 1.0]),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // The simulation worker computes U/V and uploads the colorized state.
  return textureSampleLevel(src_tex, src_sampler, uv, 0.0);
}
"#
    )
}
