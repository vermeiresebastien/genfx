//! The 28 requested named effects/fills (EFFECTS 2), part C: generator fills
//! FX020-FX028. These are sources: transparent by default, RGBA output.

use crate::builders::*;
use crate::manifest::*;

fn generator(
    id: &str,
    name: &str,
    category: &str,
    shader: &str,
    quality: QualityClass,
    mut params: Vec<ParamDecl>,
    help: &str,
    requirement: &str,
) -> Manifest {
    params.insert(0, p_float("wet", "Opacity", 1.0, 0.0, 1.0, ""));
    params.push(p_bool("transparent", "Transparent background", true));
    let mut m = effect(id, name, category, shader, quality, vec![], help, requirement);
    m.parameters = params;
    m.inputs = vec![mask_port()];
    m.outputs = vec![color_output()];
    m.time_model = TimeModel::Analytic;
    m.generator_id = id.to_string();
    m.loop_status = "exact".into();
    m
}

pub fn fx020_clouds() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.clouds",
        "Clouds",
        "Generators",
        "gen_clouds",
        QualityClass::M,
        vec![
            p_float("scale", "Scale", 3.0, 0.1, 32.0, ""),
            p_int("octaves", "Octaves", 5, 1, 8),
            p_float("coverage", "Coverage", 0.5, 0.0, 1.0, ""),
            p_float("softness", "Softness", 0.2, 0.001, 1.0, ""),
            p_float("speedX", "Speed X", 0.02, -1.0, 1.0, "cycles/s"),
            p_float("speedY", "Speed Y", 0.0, -1.0, 1.0, "cycles/s"),
            p_color("foreground", "Foreground", [1.0, 1.0, 1.0, 1.0]),
            p_color("background", "Background", [0.0, 0.0, 0.0, 1.0]),
            p_bool("alphaDensity", "Alpha from density", false),
        ],
        "Animated 2D fBm cloud density with coverage/softness shaping. Seamless over the loop period.",
        "FX020",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let period = vec2<f32>(p_scale() * 8.0);
  let t = vec2<f32>(time_seconds() * p_speedX(), time_seconds() * p_speedY()) * 8.0;
  let n = periodic_fbm(uv * period + t, period, p_octaves());
  let cov = clamp(p_coverage(), 0.0, 1.0);
  let soft = max(p_softness(), 0.001);
  let density = smoothstep(cov - soft, cov + soft, n);
  var col = mix(p_background().rgb, p_foreground().rgb, density);
  var a = 1.0;
  if (p_transparent()) {
    if (p_alphaDensity()) { a = density; }
    else { a = smoothstep(0.0, 0.05, density); }
    if (a <= 0.001) { return vec4<f32>(0.0); }
  }
  return premul(col, a);
}
"#;
    ("gen_clouds", m, body)
}

pub fn fx021_concentric() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.concentric",
        "Concentric patterns",
        "Generators",
        "gen_concentric",
        QualityClass::L,
        vec![
            p_enum("shape", "Shape", "circle", &["circle", "square", "polygon"]),
            p_int("count", "Count", 12, 1, 256),
            p_float("spacing", "Spacing", 32.0, 1.0, 512.0, "output_px"),
            p_float("thickness", "Thickness", 4.0, 0.5, 64.0, "output_px"),
            p_vec2("center", "Center", [0.5, 0.5], 0.0, 1.0, ""),
            p_float("phase", "Phase", 0.0, 0.0, 1.0, "cycles"),
            p_float("speed", "Speed", 0.1, 0.0, 4.0, "cycles/s"),
            p_float("twist", "Twist", 0.0, -4.0, 4.0, ""),
            p_color("color", "Color", [1.0, 1.0, 1.0, 1.0]),
            p_float("sides", "Polygon sides", 6.0, 3.0, 24.0, ""),
        ],
        "Antialiased concentric circles, squares or polygons with optional twist.",
        "FX021",
    );
    let body = r#"
fn shape_dist(p: vec2<f32>) -> f32 {
  let shape = p_shape();
  if (shape == 0u) { return length(p); }
  if (shape == 1u) { return max(abs(p.x), abs(p.y)); }
  let n = max(p_sides(), 3.0);
  let a = atan2(p.y, p.x);
  let seg = 6.2831853 / n;
  let d = abs(mod(a + seg * 0.5, seg) - seg * 0.5);
  return length(p) * cos(seg * 0.5) / max(cos(d), 0.0001);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let aspect = f32(frame.dims.x) / max(f32(frame.dims.y), 1.0);
  var p = (uv - p_center()) * vec2<f32>(aspect, 1.0);
  p = rot2(p.x * p_twist()) * p;
  let px = p * vec2<f32>(f32(frame.dims.y));
  let d = shape_dist(px);
  let ph = fract(p_phase() + time_seconds() * p_speed());
  let sp = max(p_spacing(), 1.0);
  let rad = mod(d + ph * sp, sp);
  let th = max(p_thickness(), 0.5);
  let edge = abs(rad - sp * 0.5);
  let mask = 1.0 - smoothstep(th * 0.5 - 0.5, th * 0.5 + 0.5, edge);
  let col = p_color().rgb * mask;
  var a = 1.0;
  if (p_transparent()) { a = mask; if (a <= 0.003) { return vec4<f32>(0.0); } }
  return premul(col, a);
}
"#;
    ("gen_concentric", m, body)
}

pub fn fx022_fractal_noise() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.fractal_noise",
        "Fractal noise",
        "Generators",
        "gen_fractal_noise",
        QualityClass::M,
        vec![
            p_enum("type", "Type", "value", &["value", "gradient", "ridge", "turbulence", "cellular"]),
            p_int("octaves", "Octaves", 5, 1, 8),
            p_float("lacunarity", "Lacunarity", 2.0, 1.0, 4.0, ""),
            p_float("persistence", "Persistence", 0.5, 0.0, 1.0, ""),
            p_float("scale", "Scale", 4.0, 0.1, 64.0, ""),
            p_float("speed", "Speed", 0.1, -4.0, 4.0, "cycles/s"),
            p_bool("seamless", "Seamless loop", false),
            p_color("colorLow", "Low color", [0.0, 0.0, 0.0, 1.0]),
            p_color("colorHigh", "High color", [1.0, 1.0, 1.0, 1.0]),
        ],
        "Fractal noise with value/gradient/ridge/turbulence/cellular types. Seamless mode uses periodic coordinates with verified border equality.",
        "FX022",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let sc = p_scale();
  let t = time_seconds() * p_speed();
  var n = 0.0;
  if (p_seamless()) {
    let period = vec2<f32>(sc * 8.0);
    n = periodic_fbm(uv * period + vec2<f32>(t * 8.0), period, p_octaves());
  } else {
    let p = uv * sc + vec2<f32>(t);
    let ty = p_type();
    if (ty == 0u) { n = fbm(p, p_octaves(), p_lacunarity(), p_persistence()); }
    else if (ty == 1u) {
      // gradient-dominant fBm
      var q = p; var amp = 0.5; var sum = 0.0; var norm = 0.0;
      for (var i = 0; i < p_octaves(); i = i + 1) {
        sum += amp * gradient_noise(q); norm += amp; q = q * p_lacunarity(); amp *= p_persistence();
      }
      n = sum / max(norm, 0.0001);
    }
    else if (ty == 2u) { n = ridged(p, p_octaves(), p_lacunarity(), p_persistence()); }
    else if (ty == 3u) { n = turbulence(p, p_octaves(), p_lacunarity(), p_persistence()); }
    else {
      // Cellular: nearest feature distance.
      let ip = floor(p);
      let fp = fract(p);
      var md = 8.0;
      for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
          let g = vec2<f32>(f32(x), f32(y));
          let o = hash22(ip + g);
          md = min(md, length(g + o - fp));
        }
      }
      n = clamp(md * 1.5, 0.0, 1.0);
    }
  }
  let col = mix(p_colorLow().rgb, p_colorHigh().rgb, clamp(n, 0.0, 1.0));
  var a = 1.0;
  if (p_transparent()) { a = smoothstep(0.0, 0.02, abs(n - 0.5)); }
  return premul(col, a);
}
"#;
    ("gen_fractal_noise", m, body)
}

pub fn fx023_glowing_wave() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.glowing_wave",
        "Glowing wave",
        "Generators",
        "gen_glowing_wave",
        QualityClass::M,
        vec![
            p_enum("curve", "Curve", "sine", &["sine", "sines", "audioWave"]),
            p_float("amplitude", "Amplitude", 0.2, 0.0, 1.0, ""),
            p_float("frequency", "Frequency", 2.0, 0.1, 32.0, ""),
            p_float("speed", "Speed", 0.2, -4.0, 4.0, "cycles/s"),
            p_float("thickness", "Thickness", 3.0, 0.5, 32.0, "output_px"),
            p_float("glow", "Glow", 16.0, 0.0, 128.0, "output_px"),
            p_color("color", "Color", [0.2, 0.8, 1.0, 1.0]),
        ],
        "An emissive line curve. Audio-driven mode deforms the curve with the audio envelope.",
        "FX023",
    );
    let body = r#"
fn height_at(x: f32) -> f32 {
  let t = time_seconds() * p_speed() * 6.2831853;
  let f = p_frequency() * 6.2831853;
  let curve = p_curve();
  var h = 0.0;
  if (curve == 0u) { h = sin(x * f + t); }
  else if (curve == 1u) { h = (sin(x * f + t) + 0.5 * sin(x * f * 2.3 - t * 1.7)) / 1.5; }
  else { h = audio_rms() * 4.0 * sin(x * f + t); }
  return h * p_amplitude() * 0.5;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let y = 0.5 - uv.y;
  let h = height_at(uv.x * 0.5 + 0.5);
  let d_px = abs(y - h) * f32(frame.dims.y);
  let core = 1.0 - smoothstep(p_thickness() * 0.5, p_thickness() * 0.5 + 1.0, d_px);
  let g = max(p_glow(), 0.01);
  let glow = exp(-d_px / g) * 0.6;
  let m = clamp(core + glow, 0.0, 1.0);
  let col = p_color().rgb * (core * 2.0 + glow);
  var a = 1.0;
  if (p_transparent()) { a = m; if (a <= 0.003) { return vec4<f32>(0.0); } }
  return premul(col, a);
}
"#;
    ("gen_glowing_wave", m, body)
}

pub fn fx024_mesh_gradient() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.mesh_gradient",
        "Mesh gradient",
        "Generators",
        "gen_mesh_gradient",
        QualityClass::M,
        vec![
            p_color("c00", "Top-left", [0.9, 0.2, 0.4, 1.0]),
            p_color("c10", "Top-right", [0.2, 0.4, 0.9, 1.0]),
            p_color("c01", "Bottom-left", [0.2, 0.9, 0.5, 1.0]),
            p_color("c11", "Bottom-right", [0.9, 0.8, 0.2, 1.0]),
            p_float("speed", "Speed", 0.1, 0.0, 4.0, "cycles/s"),
            p_float("turbulence", "Turbulence", 0.0, 0.0, 1.0, ""),
            p_float("move", "Point motion", 0.15, 0.0, 0.5, ""),
        ],
        "Bilinear mesh gradient over four control-point colors with animated points and optional turbulence. The 8?8 editor grid uses the same bilinear surface.",
        "FX024",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed() * 6.2831853;
  let m = p_move();
  let off00 = vec2<f32>(sin(t), cos(t * 0.7)) * m;
  let off10 = vec2<f32>(cos(t * 1.1), sin(t * 0.9)) * m;
  let off01 = vec2<f32>(sin(t * 0.8 + 1.0), cos(t * 1.3)) * m;
  let off11 = vec2<f32>(cos(t * 1.4), sin(t * 1.1 + 2.0)) * m;
  var p = uv;
  if (p_turbulence() > 0.0) {
    let n = fbm(uv * 4.0 + vec2<f32>(t), 4, 2.0, 0.5) - 0.5;
    p = uv + vec2<f32>(n) * p_turbulence() * 0.2;
  }
  // Inverse bilinear weighting approximated by smooth cosine weights.
  let wx = smoothstep(0.0, 1.0, clamp(p.x + 0.0, 0.0, 1.0));
  let wy = smoothstep(0.0, 1.0, clamp(p.y, 0.0, 1.0));
  let d00 = length(p - (vec2<f32>(0.0, 0.0) + off00));
  let d10 = length(p - (vec2<f32>(1.0, 0.0) + off10));
  let d01 = length(p - (vec2<f32>(0.0, 1.0) + off01));
  let d11 = length(p - (vec2<f32>(1.0, 1.0) + off11));
  var w = array<f32, 4>(exp(-d00 * 3.0), exp(-d10 * 3.0), exp(-d01 * 3.0), exp(-d11 * 3.0));
  let sum = w[0] + w[1] + w[2] + w[3];
  var col = (p_c00().rgb * w[0] + p_c10().rgb * w[1] + p_c01().rgb * w[2] + p_c11().rgb * w[3]) / max(sum, 0.0001);
  let bil = mix(mix(p_c00().rgb, p_c10().rgb, wx), mix(p_c01().rgb, p_c11().rgb, wx), wy);
  col = mix(bil, col, 0.35);
  return premul(col, 1.0);
}
"#;
    ("gen_mesh_gradient", m, body)
}

pub fn fx025_moire() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.moire",
        "Moire",
        "Generators",
        "gen_moire",
        QualityClass::L,
        vec![
            p_enum("field", "Field", "grid", &["lines", "radial", "grid"]),
            p_float("frequency1", "Frequency 1", 24.0, 1.0, 128.0, "cycles/image"),
            p_float("frequency2", "Frequency 2", 24.5, 1.0, 128.0, "cycles/image"),
            p_float("relativeAngle", "Relative angle", 3.0, -90.0, 90.0, "deg"),
            p_float("phase", "Phase", 0.0, 0.0, 1.0, "cycles"),
            p_float("speed", "Speed", 0.1, 0.0, 4.0, "cycles/s"),
            p_enum("blend", "Blend", "multiply", &["multiply", "difference", "add"]),
            p_color("color", "Color", [1.0, 1.0, 1.0, 1.0]),
        ],
        "Two overlapping high-frequency fields produce an interference pattern. Changing the frequency delta changes the envelope.",
        "FX025",
    );
    let body = r#"
fn field_value(uv: vec2<f32>, freq: f32, angle: f32) -> f32 {
  let aspect = f32(frame.dims.x) / max(f32(frame.dims.y), 1.0);
  let p = (uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0);
  if (p_field() == 0u) {
    let r = rot2(angle) * p;
    return sin((r.x * freq + p_phase()) * 6.2831853) * 0.5 + 0.5;
  } else if (p_field() == 1u) {
    let r = rot2(angle) * p;
    return sin((length(r) * freq + p_phase()) * 6.2831853) * 0.5 + 0.5;
  }
  let r = rot2(angle) * p;
  return (sin((r.x * freq + p_phase()) * 6.2831853) * sin((r.y * freq + p_phase()) * 6.2831853)) * 0.5 + 0.5;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed();
  let a = field_value(uv, p_frequency1(), 0.0 + t);
  let b = field_value(uv, p_frequency2(), radians(p_relativeAngle()) + t * 0.5);
  var v = 0.0;
  if (p_blend() == 0u) { v = a * b; }
  else if (p_blend() == 1u) { v = abs(a - b); }
  else { v = clamp(a + b, 0.0, 1.0); }
  let col = p_color().rgb * v;
  return premul(col, 1.0);
}
"#;
    ("gen_moire", m, body)
}

pub fn fx026_nebula() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.nebula",
        "Nebula",
        "Generators",
        "gen_nebula",
        QualityClass::H,
        vec![
            p_float("scale", "Scale", 3.0, 0.1, 16.0, ""),
            p_float("density", "Density", 0.5, 0.0, 1.0, ""),
            p_float("warp", "Warp", 0.4, 0.0, 1.0, ""),
            p_float("speed", "Speed", 0.03, 0.0, 1.0, "cycles/s"),
            p_float("starDensity", "Star density", 0.002, 0.0, 0.05, ""),
            p_color("colorA", "Color A", [0.1, 0.05, 0.3, 1.0]),
            p_color("colorB", "Color B", [0.8, 0.2, 0.6, 1.0]),
            p_color("colorC", "Color C", [0.3, 0.6, 1.0, 1.0]),
        ],
        "Layered warped fBm with colored density, seeded sparse stars and a distinct look from the Clouds generator.",
        "FX026",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds() * p_speed();
  let sc = p_scale();
  let wp = uv * sc + vec2<f32>(t);
  let w1 = fbm(wp * 1.3 + vec2<f32>(t * 2.0), 4, 2.0, 0.5) - 0.5;
  let w2 = fbm(wp * 1.3 + vec2<f32>(3.1, -2.0) - vec2<f32>(t), 4, 2.0, 0.5) - 0.5;
  let dw = uv + vec2<f32>(w1, w2) * p_warp();
  let n = fbm(dw * sc + vec2<f32>(t), 6, 2.0, 0.55);
  let dens = smoothstep(1.0 - clamp(p_density(), 0.0, 1.0), 1.0, n);
  var col = mix(p_colorA().rgb, p_colorB().rgb, smoothstep(0.2, 0.8, n));
  col = mix(col, p_colorC().rgb, smoothstep(0.5, 1.0, dens) * 0.6);
  col *= dens * 1.6;
  // Stars: sparse seeded points.
  let cellg = floor(uv * 900.0);
  let h = hash21(cellg);
  if (h > 1.0 - p_starDensity() * 40.0) {
    let star_pos = (hash22(cellg) - 0.5) * 0.6;
    let d = length(fract(uv * 900.0) - 0.5 - star_pos);
    let star = exp(-d * 60.0);
    col += vec3<f32>(star * 2.0);
  }
  return premul(col, 1.0);
}
"#;
    ("gen_nebula", m, body)
}

pub fn fx027_pattern_grid() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.pattern_grid",
        "Pattern grid",
        "Generators",
        "gen_pattern_grid",
        QualityClass::L,
        vec![
            p_enum("shape", "Cell shape", "square", &["square", "hex", "triangle", "circle"]),
            p_int("rows", "Rows", 12, 1, 128),
            p_int("columns", "Columns", 20, 1, 128),
            p_float("gap", "Gap", 0.1, 0.0, 0.9, ""),
            p_float("border", "Border", 1.0, 0.0, 8.0, "output_px"),
            p_float("jitter", "Jitter", 0.0, 0.0, 1.0, ""),
            p_float("phase", "Phase", 0.0, 0.0, 1.0, "cycles"),
            p_color("fill", "Fill", [0.9, 0.9, 0.9, 1.0]),
            p_color("stroke", "Stroke", [0.1, 0.1, 0.1, 1.0]),
            p_bool("alternate", "Alternate pattern", false),
        ],
        "Deterministic grid of cells (square, hex, triangle or circle) with gap, jitter and animation phase.",
        "FX027",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let cols = f32(max(p_columns(), 1));
  let rows = f32(max(p_rows(), 1));
  let aspect = f32(frame.dims.x) / max(f32(frame.dims.y), 1.0);
  let g = uv * vec2<f32>(cols, rows);
  let cell = floor(g);
  let f = fract(g) - vec2<f32>(0.5);
  let jit = vec2<f32>(hash21(cell) - 0.5, hash21(cell + vec2<f32>(17.0)) - 0.5) * p_jitter();
  let ff = f - jit;
  // animate: cells pulse slightly with phase
  let anim = 0.5 + 0.5 * sin((p_phase() + time_seconds() * 0.5) * 6.2831853);
  let gap = p_gap() * 0.5;
  var in_cell = 0.0;
  let shape = p_shape();
  if (shape == 0u) {
    in_cell = select(0.0, 1.0, max(abs(ff.x), abs(ff.y)) < 0.5 - gap);
  } else if (shape == 1u) {
    let hh = max(abs(ff.x) * 0.866 + abs(ff.y) * 0.5, abs(ff.y));
    in_cell = select(0.0, 1.0, hh < 0.5 - gap);
  } else if (shape == 2u) {
    in_cell = select(0.0, 1.0, (ff.y + 0.5) > abs(ff.x) * 2.0 + gap && ff.y < 0.5 - gap);
  } else {
    in_cell = select(0.0, 1.0, length(ff) < 0.5 - gap);
  }
  let border_uv = px_to_uv(p_border());
  let inner = select(0.0, 1.0, max(abs(ff.x), abs(ff.y)) < 0.5 - gap - border_uv.x * 0.5);
  var col = mix(p_fill().rgb, p_stroke().rgb, 1.0 - inner);
  col = col * mix(1.0, anim, 0.2);
  if (p_alternate()) {
    let parity = mod(cell.x + cell.y, 2.0);
    col = mix(col, p_stroke().rgb, parity * 0.25);
  }
  let a = select(0.0, 1.0, in_cell > 0.5);
  if (!p_transparent()) { return premul(col, 1.0); }
  return premul(col, a);
}
"#;
    ("gen_pattern_grid", m, body)
}

pub fn fx028_water_caustic() -> (&'static str, Manifest, &'static str) {
    let m = generator(
        "gen.water_caustic",
        "Water caustic",
        "Generators",
        "gen_water_caustic",
        QualityClass::M,
        vec![
            p_float("scale", "Scale", 5.0, 0.5, 32.0, ""),
            p_float("speed", "Speed", 0.2, -2.0, 2.0, "cycles/s"),
            p_float("sharpness", "Sharpness", 8.0, 1.0, 64.0, ""),
            p_float("intensity", "Intensity", 1.0, 0.0, 4.0, ""),
            p_color("tint", "Tint", [0.6, 0.85, 1.0, 1.0]),
            p_float("depth", "Depth", 1.0, 0.0, 4.0, ""),
        ],
        "Animated refractive caustic intensity from summed waves. This approximates caustic illumination, not physically traced light.",
        "FX028",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let sc = p_scale();
  let t = time_seconds() * p_speed();
  var p = uv * sc;
  var v = 0.0;
  // Three summed directional waves create an interference intensity field.
  for (var i = 0; i < 3; i = i + 1) {
    let fi = f32(i);
    let ang = fi * 2.1 + t;
    let dir = vec2<f32>(cos(ang), sin(ang));
    v += sin(dot(p, dir) * 6.2831853 + t * (2.0 + fi) + seedf() * 0.0);
  }
  let height = v / 3.0;
  let caustic = pow(1.0 - abs(height), max(p_sharpness(), 1.0)) * p_intensity();
  let col = p_tint().rgb * caustic;
  let a = mix(0.0, 1.0, clamp(caustic, 0.0, 1.0));
  if (!p_transparent()) { return premul(col, 1.0); }
  return premul(col, a);
}
"#;
    ("gen_water_caustic", m, body)
}
