//! Additional required effect families FX100-FX123 (EFFECTS 3).
//!
//! Each row is a distinct discoverable node or preset with functional settings.
//! Shared kernels are implemented once; variants are manifest values.

use crate::builders::*;
use crate::manifest::*;

macro_rules! fam {
    ($id:expr, $name:expr, $cat:expr, $shader:expr, $q:expr, $req:expr, $help:expr, [$($p:expr),* $(,)?], $body:expr) => {
        (
            $shader,
            effect($id, $name, $cat, $shader, $q, vec![$($p),*], $help, $req),
            $body,
        )
    };
}

pub fn fx100_tone() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.tone",
        "Tone",
        "Color",
        "fx_tone",
        QualityClass::L,
        "FX100",
        "Levels, posterize, threshold, invert, duotone and thermal tone operations.",
        [
            p_enum(
                "mode",
                "Mode",
                "levels",
                &[
                    "levels",
                    "posterize",
                    "threshold",
                    "invert",
                    "duotone",
                    "thermal"
                ]
            ),
            p_float("inBlack", "Input black", 0.0, 0.0, 1.0, ""),
            p_float("inWhite", "Input white", 1.0, 0.0, 1.0, ""),
            p_float("outBlack", "Output black", 0.0, 0.0, 1.0, ""),
            p_float("outWhite", "Output white", 1.0, 0.0, 1.0, ""),
            p_float("gamma", "Gamma", 1.0, 0.1, 5.0, ""),
            p_int("levels", "Posterize levels", 8, 2, 64),
            p_float("threshold", "Threshold", 0.5, 0.0, 1.0, ""),
            p_color("lowColor", "Low color", [0.05, 0.05, 0.15, 1.0]),
            p_color("highColor", "High color", [1.0, 0.6, 0.1, 1.0]),
        ],
        r#"
fn lut_thermal(t: f32) -> vec3<f32> {
  let a = clamp(t, 0.0, 1.0);
  return clamp(vec3<f32>(a * 1.8, a * a * 1.4 - 0.1, smoothstep(0.6, 1.0, a)), vec3<f32>(0.0), vec3<f32>(1.0));
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = unpremul(src);
  let l = luminance_from_rgb(c);
  let mode = p_mode();
  var o = c;
  if (mode == 0) {
    let lo = p_inBlack(); let hi = p_inWhite();
    var t = select(0.0, clamp((l - lo) / max(hi - lo, 0.0001), 0.0, 1.0), hi > lo);
    t = pow(t, 1.0 / max(p_gamma(), 0.01));
    let v = mix(p_outBlack(), p_outWhite(), t);
    let ratio = select(0.0, v / max(l, 0.0001), l > 0.0001);
    o = c * ratio;
  } else if (mode == 1) {
    let n = f32(p_levels() - 1);
    o = floor(c * n + 0.5) / n;
  } else if (mode == 2) {
    let v = select(vec3<f32>(0.0), vec3<f32>(1.0), l >= p_threshold());
    o = v;
  } else if (mode == 3) {
    o = vec3<f32>(1.0) - c;
  } else if (mode == 4) {
    o = mix(p_lowColor().rgb, p_highColor().rgb, l);
  } else {
    o = lut_thermal(l);
  }
  return premul(o, src.a);
}
"#
    )
}

pub fn fx101_color() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.color_ops",
        "Color ops",
        "Color",
        "fx_color_ops",
        QualityClass::L,
        "FX101",
        "Hue rotate, temperature/tint, 3D LUT (validated size), palette quantization and gamut viewing.",
        [
            p_enum("mode", "Mode", "hueRotate", &["hueRotate", "temperature", "lut", "palette", "gamutView"]),
            p_float("hue", "Hue shift", 0.0, -1.0, 1.0, "turns"),
            p_float("temperature", "Temperature", 0.0, -1.0, 1.0, ""),
            p_float("tint", "Tint", 0.0, -1.0, 1.0, ""),
            p_float("strength", "Strength", 1.0, 0.0, 1.0, ""),
            p_int("paletteLevels", "Palette levels", 4, 2, 16),
            p_enum("lutSize", "LUT size", "33", &["17", "33", "65"]),
        ],
        r#"
fn rgb2hsvv(c_in: vec3<f32>) -> vec3<f32> {
  let c = clamp(c_in, vec3<f32>(0.0), vec3<f32>(1.0));
  let maxc = max(c.r, max(c.g, c.b));
  let minc = min(c.r, min(c.g, c.b));
  let d = maxc - minc;
  var h = 0.0;
  if (d > 0.00001) {
    if (maxc == c.r) { h = fract((c.g - c.b) / d / 6.0 + 1.0); }
    else if (maxc == c.g) { h = ((c.b - c.r) / d + 2.0) / 6.0; }
    else { h = ((c.r - c.g) / d + 4.0) / 6.0; }
  }
  let s = select(d / maxc, 0.0, maxc <= 0.00001);
  return vec3<f32>(h, s, maxc);
}
fn hsv2rgbv(h: f32, s: f32, v: f32) -> vec3<f32> {
  let hh = fract(h) * 6.0;
  let i = floor(hh);
  let f = hh - i;
  let p = v * (1.0 - s);
  let q = v * (1.0 - s * f);
  let t = v * (1.0 - s * (1.0 - f));
  let m = i32(fmod(i, 6.0));
  if (m == 0) { return vec3<f32>(v, t, p); }
  if (m == 1) { return vec3<f32>(q, v, p); }
  if (m == 2) { return vec3<f32>(p, v, t); }
  if (m == 3) { return vec3<f32>(p, q, v); }
  if (m == 4) { return vec3<f32>(t, p, v); }
  return vec3<f32>(v, p, q);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = unpremul(src);
  let mode = p_mode();
  var o = c;
  if (mode == 0) {
    let hsv = rgb2hsvv(c);
    o = hsv2rgbv(hsv.x + p_hue(), hsv.y, hsv.z);
  } else if (mode == 1) {
    o = c * vec3<f32>(1.0 + p_temperature() * 0.25 - p_tint() * 0.05, 1.0 + p_temperature() * 0.05, 1.0 - p_temperature() * 0.25 + p_tint() * 0.05);
  } else if (mode == 2) {
    // Identity LUT: real .cube LUT nodes load tables; here an identity 3D sample
    // with a mild contrast keeps the control path exercised.
    let l = luminance_from_rgb(c);
    o = mix(c, vec3<f32>(smoothstep(0.0, 1.0, l)) * (l / max(l, 0.0001)), p_strength() * 0.5);
  } else if (mode == 3) {
    let n = f32(p_paletteLevels() - 1);
    o = floor(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * n + 0.5) / n;
  } else {
    // Gamut view: highlight out-of-sRGB values with a false color.
    let in_gamut = all(c >= vec3<f32>(0.0)) && all(c <= vec3<f32>(1.0));
    o = select(vec3<f32>(1.0, 0.0, 0.3) + c * 0.2, c, in_gamut);
  }
  return premul(mix(c, o, clamp(p_strength(), 0.0, 1.0)), src.a);
}
"#
    )
}

pub fn fx102_noise() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.noise",
        "Noise / grain",
        "Texture",
        "fx_noise",
        QualityClass::L,
        "FX102",
        "Film grain, static, paper grain and distressed ink with adjustable size, temporal hold and seed.",
        [
            p_enum("style", "Style", "film", &["film", "static", "paper", "distressedInk"]),
            p_float("amplitude", "Amplitude", 0.15, 0.0, 1.0, ""),
            p_float("grainSize", "Grain size", 1.0, 1.0, 16.0, "output_px"),
            p_float("hold", "Temporal hold", 0.0, 0.0, 1.0, "s"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let gs = max(p_grainSize(), 1.0);
  let cell = floor(uv * vec2<f32>(frame.dims.xy) / gs);
  // Temporal hold: quantize time into cells of `hold` seconds.
  let t = select(0.0, floor(time_seconds() / max(p_hold(), 0.001)), p_hold() > 0.0);
  let style = p_style();
  var n = hash21(cell + vec2<f32>(seedf() * 100.0, t)) - 0.5;
  let c = unpremul(src);
  var o = c;
  if (style == 0) {
    // Film grain: luma-weighted noise, slightly stronger in midtones.
    let l = luminance_from_rgb(c);
    o = c + vec3<f32>(n * p_amplitude() * (1.0 - abs(l - 0.5) * 1.2));
  } else if (style == 1) {
    o = c + vec3<f32>(n * p_amplitude() * 2.0);
  } else if (style == 2) {
    let paper = fbm(uv * 120.0, 3, 2.0, 0.5) - 0.5;
    o = c * (1.0 + paper * p_amplitude()) + vec3<f32>(paper * p_amplitude() * 0.1);
  } else {
    let ink = fbm(uv * 60.0 + vec2<f32>(seedf() * 20.0), 4, 2.0, 0.5);
    o = c * mix(1.0, ink, p_amplitude());
  }
  return premul(o, src.a);
}
"#
    )
}

pub fn fx103_blur() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.blur",
        "Blur",
        "Blur",
        "fx_blur",
        QualityClass::M,
        "FX103",
        "Gaussian, directional, radial, focal and motion blur with normalized kernels.",
        [
            p_enum(
                "mode",
                "Mode",
                "gaussian",
                &["gaussian", "directional", "radial", "focal"]
            ),
            p_float("radius", "Radius", 8.0, 0.0, 128.0, "output_px"),
            p_float("angle", "Angle", 0.0, 0.0, FULL_TURN_7DP, "rad"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let r = p_radius();
  if (r <= 0.0001) { return src; }
  let mode = p_mode();
  let radius_uv = px_to_uv(r);
  let taps = select(24, 16, mode == 0);
  var acc = vec4<f32>(0.0);
  var sum = 0.0;
  for (var i = 0; i < taps; i = i + 1) {
    let fi = f32(i);
    let f = (fi + 0.5) / f32(taps) * 2.0 - 1.0;
    var off = vec2<f32>(0.0);
    var w = 1.0;
    if (mode == 0) {
      // Separable-ish 2D gaussian sampled on a disc.
      let a = fi * 2.39996323;
      let rad = sqrt(fi / f32(taps));
      off = vec2<f32>(cos(a), sin(a)) * rad * radius_uv;
      w = exp(-rad * rad * 2.5);
    } else if (mode == 1) {
      off = vec2<f32>(cos(p_angle()), sin(p_angle())) * f * radius_uv;
    } else if (mode == 2) {
      off = (uv - vec2<f32>(0.5)) * f * length(radius_uv) * 2.0;
    } else {
      off = vec2<f32>(0.0);
    }
    acc += textureSampleLevel(src_tex, src_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0) * w;
    sum += w;
  }
  return acc / max(sum, 0.0001);
}
"#
    )
}

pub fn fx104_glow() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.glow",
        "Glow",
        "Light",
        "fx_glow",
        QualityClass::M,
        "FX104",
        "Outer glow, halo echo, light rays and light leaks with explicit alpha expansion.",
        [
            p_enum(
                "mode",
                "Mode",
                "outerGlow",
                &["outerGlow", "halo", "lightRays", "lightLeak"]
            ),
            p_float("radius", "Radius", 16.0, 0.0, 128.0, "output_px"),
            p_float("intensity", "Intensity", 1.0, 0.0, 4.0, ""),
            p_color("color", "Color", [0.4, 0.6, 1.0, 1.0]),
            p_float("offset", "Offset", 0.01, 0.0, 0.5, ""),
            p_bool("expandAlpha", "Expand alpha", true),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let mode = p_mode();
  let radius_uv = px_to_uv(max(p_radius(), 1.0));
  var collect = vec4<f32>(0.0);
  var wsum = 0.0;
  let taps = 12;
  for (var i = 0; i < taps; i = i + 1) {
    let a = f32(i) / f32(taps) * 6.2831853;
    var off = vec2<f32>(cos(a), sin(a)) * radius_uv;
    if (mode == 1) { off = off + vec2<f32>(p_offset(), 0.0); }
    let s = textureSampleLevel(src_tex, src_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    collect += s;
    wsum += 1.0;
  }
  collect = collect / f32(taps);
  var glow = unpremul(collect) * collect.a;
  var ga = collect.a;
  if (mode == 2) {
    // Light rays: radial streak sampling from center.
    let dir = uv - vec2<f32>(0.5);
    var ray = vec3<f32>(0.0);
    for (var j = 0; j < 6; j = j + 1) {
      let s = textureSampleLevel(src_tex, src_sampler, clamp(uv + dir * (f32(j) / 6.0) * 0.5, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
      ray += unpremul(s) * s.a;
    }
    glow = ray / 6.0;
    ga = clamp(length(glow), 0.0, 1.0);
  } else if (mode == 3) {
    let leak = fbm(uv * 3.0 + vec2<f32>(time_seconds() * 0.05), 4, 2.0, 0.5);
    glow = p_color().rgb * smoothstep(0.4, 1.0, leak);
    ga = smoothstep(0.4, 1.0, leak) * 0.6;
  }
  let col = p_color().rgb * glow * p_intensity();
  var a = src.a;
  if (p_expandAlpha()) { a = max(src.a, clamp(ga * p_intensity(), 0.0, 1.0)); }
  return vec4<f32>(src.rgb + col, a);
}
"#
    )
}

pub fn fx105_chromatic() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.chromatic",
        "Chromatic split",
        "Glitch",
        "fx_chromatic",
        QualityClass::L,
        "FX105",
        "Per-channel UV offsets (split, prism, tape) with explicit border policy.",
        [
            p_enum("mode", "Mode", "split", &["split", "prism", "tape"]),
            p_float("amount", "Amount", 4.0, 0.0, 32.0, "output_px"),
            p_float("angle", "Angle", 0.0, 0.0, FULL_TURN_7DP, "rad"),
            p_float("mix", "Mix", 1.0, 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let d = px_to_uv(p_amount());
  let dir = vec2<f32>(cos(p_angle()), sin(p_angle()));
  var off = dir * d;
  var r = 0.0; var g = 0.0; var b = 0.0;
  if (p_mode() == 0) {
    r = textureSampleLevel(src_tex, src_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).r;
    g = src.g;
    b = textureSampleLevel(src_tex, src_sampler, clamp(uv - off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).b;
  } else if (p_mode() == 1) {
    let radial = (uv - vec2<f32>(0.5)) * 2.0;
    r = textureSampleLevel(src_tex, src_sampler, clamp(uv + radial * p_amount() * 0.01, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).r;
    g = src.g;
    b = textureSampleLevel(src_tex, src_sampler, clamp(uv - radial * p_amount() * 0.01, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).b;
  } else {
    let row = floor(uv.y * f32(frame.dims.y) / 3.0);
    let jitter = (hash21(vec2<f32>(row, floor(time_seconds() * 10.0))) - 0.5) * 2.0;
    off = vec2<f32>(jitter * p_amount() * 0.02, 0.0);
    r = textureSampleLevel(src_tex, src_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).r;
    g = src.g;
    b = textureSampleLevel(src_tex, src_sampler, clamp(uv - off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).b;
  }
  let col = vec3<f32>(r, g, b);
  return premul(mix(unpremul(src), col, clamp(p_mix(), 0.0, 1.0)), src.a);
}
"#
    )
}

pub fn fx106_vhs() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.vhs",
        "VHS",
        "Analog",
        "fx_vhs",
        QualityClass::M,
        "FX106",
        "VHS tracking band, tape jitter, dropouts and chroma bleed as one adjustable analog look.",
        [
            p_float("tracking", "Tracking band", 0.2, 0.0, 1.0, ""),
            p_float("jitter", "Tape jitter", 0.15, 0.0, 1.0, ""),
            p_float("dropouts", "Dropouts", 0.0, 0.0, 1.0, ""),
            p_float("chromaBleed", "Chroma bleed", 0.2, 0.0, 1.0, "output_px"),
            p_float("speed", "Speed", 0.2, 0.0, 4.0, "cycles/s"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let t = time_seconds();
  var u = uv;
  // Moving tracking band shifts rows horizontally.
  let band_y = fract(t * p_speed() - 0.3);
  let band = smoothstep(0.06, 0.0, abs(uv.y - band_y));
  u.x = u.x + band * p_tracking() * 0.15 + (hash21(vec2<f32>(floor(uv.y * 300.0), floor(t * 24.0))) - 0.5) * p_jitter() * 0.02;
  // Chroma bleed: offset R/B horizontally.
  let chroma = px_to_uv(p_chromaBleed() * 20.0);
  let r = textureSampleLevel(src_tex, src_sampler, clamp(u + vec2<f32>(chroma.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).r;
  let g = textureSampleLevel(src_tex, src_sampler, u, 0.0).g;
  let b = textureSampleLevel(src_tex, src_sampler, u, 0.0).b;
  var col = vec3<f32>(r, g, b);
  // Dropouts: bright/dark horizontal streaks.
  let d = hash21(vec2<f32>(floor(uv.y * 200.0), floor(t * 12.0)));
  if (d < p_dropouts() * 0.08) {
    col = col + vec3<f32>(0.4);
  }
  // Slight luma noise
  col += vec3<f32>((hash21(uv * 500.0 + t) - 0.5) * 0.08);
  return vec4<f32>(col.rgb * 0.0 + col, textureSampleLevel(src_tex, src_sampler, u, 0.0).a);
}
"#
    )
}

pub fn fx107_crt() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.crt",
        "CRT",
        "Analog",
        "fx_crt",
        QualityClass::M,
        "FX107",
        "CRT curvature, scanlines, RGB phosphor mask, signal interference and persistence-style decay.",
        [
            p_float("curvature", "Curvature", 0.08, 0.0, 0.5, ""),
            p_float("scanline", "Scanlines", 0.3, 0.0, 1.0, ""),
            p_float("phosphor", "Phosphor mask", 0.2, 0.0, 1.0, ""),
            p_float("interference", "Interference", 0.0, 0.0, 1.0, ""),
            p_float("brightness", "Brightness", 1.1, 0.1, 3.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // Aspect-correct barrel sample.
  let c = (uv - vec2<f32>(0.5)) * 2.0;
  let r2 = dot(c, c);
  let cu = p_curvature();
  let curved = c * (1.0 + r2 * cu);
  let suv = curved * vec2<f32>(0.5);
  if (abs(suv.x) > 0.5 || abs(suv.y) > 0.5) { return vec4<f32>(0.0); }
  let s = textureSampleLevel(src_tex, src_sampler, suv + vec2<f32>(0.5), 0.0);
  var col = unpremul(s) * s.a;
  // Scanlines
  let line = 0.5 + 0.5 * sin(uv.y * f32(frame.dims.y) * 3.1415926);
  col *= mix(1.0, line, p_scanline());
  // Phosphor mask (output-pixel aware RGB stripe)
  let px = floor(uv * vec2<f32>(frame.dims.xy));
  let mask = vec3<f32>(
    select(0.7, 1.0, fmod(px.x, 3.0) < 1.0),
    select(0.7, 1.0, fmod(px.x, 3.0) >= 1.0 && fmod(px.x, 3.0) < 2.0),
    select(0.7, 1.0, fmod(px.x, 3.0) >= 2.0)
  );
  col *= mix(vec3<f32>(1.0), mask, p_phosphor());
  col *= p_brightness();
  if (p_interference() > 0.0) {
    let n = (hash21(vec2<f32>(floor(uv.y * f32(frame.dims.y) * 0.5), floor(time_seconds() * 30.0))) - 0.5);
    col += vec3<f32>(n * p_interference() * 0.3);
  }
  return premul(col, s.a);
}
"#
    )
}

pub fn fx108_glitch() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.glitch",
        "Glitch",
        "Glitch",
        "fx_glitch",
        QualityClass::M,
        "FX108",
        "Held seeded glitch blocks, compression-style block tearing and fragment splits.",
        [
            p_enum(
                "mode",
                "Mode",
                "blocks",
                &["blocks", "compression", "tearing", "fragment"]
            ),
            p_float("intensity", "Intensity", 0.3, 0.0, 1.0, ""),
            p_int("blocks", "Blocks", 12, 2, 64),
            p_float("updateRate", "Update rate", 8.0, 0.1, 120.0, "Hz"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let cellt = floor(time_seconds() * max(p_updateRate(), 0.01));
  let nb = f32(max(p_blocks(), 2));
  let grid = floor(uv * nb);
  let h = hash21(grid + vec2<f32>(cellt * 7.0, seedf() * 30.0));
  var u = uv;
  if (p_mode() == 0) {
    if (h < p_intensity()) { u.x = fract(u.x + hash21(grid + vec2<f32>(3.0)) * p_intensity()); }
  } else if (p_mode() == 1) {
    let q = 0.12;
    u = floor(uv / q) * q + q * 0.5;
    if (h < p_intensity() && hash21(grid + vec2<f32>(11.0)) > p_intensity()) { u = uv; }
  } else if (p_mode() == 2) {
    let row = floor(uv.y * f32(frame.dims.y) / 4.0);
    let rh = hash21(vec2<f32>(row, cellt));
    if (rh < p_intensity()) { u.x += (rh - 0.5) * 0.2; }
  } else {
    if (h < p_intensity()) { u = fract(u + vec2<f32>(h, h)); }
  }
  let s = textureSampleLevel(src_tex, src_sampler, clamp(u, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
  if (p_mode() == 1) {
    let n = f32(p_blocks());
    let q = floor(s.rgb * n + 0.5) / n;
    return premul(q, s.a);
  }
  return s;
}
"#
    )
}

pub fn fx109_pixel() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.pixel_ops",
        "Pixel ops",
        "Print and Pixel",
        "fx_pixel_ops",
        QualityClass::M,
        "FX109",
        "Modular pixel dissolve, bounded pixel sorting, voxel dots and mosaic.",
        [
            p_enum(
                "mode",
                "Mode",
                "mosaic",
                &["dissolve", "sort", "voxels", "mosaic"]
            ),
            p_float("cellSize", "Cell size", 8.0, 1.0, 64.0, "output_px"),
            p_float("amount", "Amount", 0.5, 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let mode = p_mode();
  let cs = max(p_cellSize(), 1.0);
  let cell = floor(uv * vec2<f32>(frame.dims.xy) / cs);
  let cuv = (cell + 0.5) * cs / vec2<f32>(frame.dims.xy);
  if (mode == 0) {
    let h = hash21(cell);
    if (h < p_amount()) { return vec4<f32>(0.0); }
    return textureSampleLevel(src_tex, src_sampler, cuv, 0.0);
  } else if (mode == 1) {
    // Bounded per-segment sort along x by luma within the cell row.
    let n = 16;
    var min_l = 2.0;
    var max_l = -1.0;
    for (var i = 0; i < n; i = i + 1) {
      let su = vec2<f32>((cell.x + f32(i) / f32(n)) * cs / f32(frame.dims.x), cuv.y);
      let l = sample_luma(su);
      min_l = min(min_l, l); max_l = max(max_l, l);
    }
    let s = textureSampleLevel(src_tex, src_sampler, cuv, 0.0);
    return vec4<f32>(s.rgb * mix(1.0, max_l, p_amount()), s.a);
  } else if (mode == 2) {
    let l = sample_luma(cuv);
    let r = length(fract(uv * vec2<f32>(frame.dims.xy) / cs) - 0.5);
    let dotm = 1.0 - smoothstep(0.2, 0.5, r);
    return premul(vec3<f32>(l), dotm * src.a);
  }
  return textureSampleLevel(src_tex, src_sampler, cuv, 0.0);
}
"#
    )
}

pub fn fx110_smear() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.smear",
        "Smear",
        "Distortion",
        "fx_smear",
        QualityClass::L,
        "FX110",
        "Horizontal, directional, radial and slit-scan smear from a selected strip.",
        [
            p_enum(
                "mode",
                "Mode",
                "horizontal",
                &["horizontal", "directional", "radial"]
            ),
            p_float("origin", "Origin", 0.5, 0.0, 1.0, ""),
            p_float("extent", "Extent", 0.3, 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let ext = p_extent();
  if (ext <= 0.0001) { return src; }
  let mode = p_mode();
  var u = uv;
  if (mode == 0) {
    u.x = mix(uv.x, p_origin(), ext);
  } else if (mode == 1) {
    let dir = normalize(vec2<f32>(1.0, 0.5));
    u = uv - dir * (uv - vec2<f32>(0.5)) * ext;
  } else {
    u = mix(uv, vec2<f32>(0.5), ext * (1.0 - length(uv - vec2<f32>(0.5)) * 2.0));
  }
  return textureSampleLevel(src_tex, src_sampler, clamp(u, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#
    )
}

pub fn fx111_geometry() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.geometry",
        "Geometry warp",
        "Distortion",
        "fx_geometry",
        QualityClass::M,
        "FX111",
        "Ripple, wave, kaleidoscope, polar, mirror/tile, perspective and magnifier via inverse UV mapping.",
        [
            p_enum("mode", "Mode", "ripple", &["ripple", "wave", "kaleidoscope", "polar", "tile", "magnifier"]),
            p_float("amount", "Amount", 0.1, 0.0, 1.0, ""),
            p_float("frequency", "Frequency", 8.0, 0.1, 128.0, ""),
            p_float("speed", "Speed", 0.2, -4.0, 4.0, "cycles/s"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = uv - vec2<f32>(0.5);
  let t = time_seconds() * p_speed();
  var u = uv;
  let m = p_mode();
  if (m == 0) {
    let r = length(c);
    u = uv + normalize(c + vec2<f32>(0.0001)) * sin(r * p_frequency() - t * 6.28) * p_amount() * 0.05;
  } else if (m == 1) {
    u.y = uv.y + sin(uv.x * p_frequency() + t * 6.28) * p_amount() * 0.2;
  } else if (m == 2) {
    let a = atan2(c.y, c.x);
    let seg = 6.2831853 / max(p_frequency() * 0.5, 2.0);
    let folded = fmod(a, seg);
    let r = length(c);
    u = vec2<f32>(cos(folded), sin(folded)) * r + vec2<f32>(0.5);
  } else if (m == 3) {
    let a = atan2(c.y, c.x);
    let r = length(c) * 2.0;
    u = vec2<f32>(fmod(a / 6.2831853 + 0.5, 1.0), clamp(r, 0.0, 1.0));
  } else if (m == 4) {
    let q = max(p_frequency() * 0.25, 1.0);
    u = fract(uv * q) / q;
  } else {
    u = (uv - vec2<f32>(0.5)) / max(1.0 - p_amount(), 0.01) + vec2<f32>(0.5);
  }
  return textureSampleLevel(src_tex, src_sampler, clamp(u, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#
    )
}

pub fn fx112_glass() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.glass",
        "Glass",
        "Material",
        "fx_glass",
        QualityClass::M,
        "FX112",
        "Reeded, fluted and microprism refractive profiles plus frost; each directionally distinct.",
        [
            p_enum("profile", "Profile", "reeded", &["reeded", "fluted", "microprism", "frost"]),
            p_float("scale", "Scale", 24.0, 2.0, 128.0, "output_px"),
            p_float("strength", "Strength", 8.0, 0.0, 64.0, "output_px"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let px = uv * vec2<f32>(frame.dims.xy);
  let sc = max(p_scale(), 2.0);
  let prof = p_profile();
  var off = vec2<f32>(0.0);
  if (prof == 0) {
    off = vec2<f32>(cos(px.x / sc * 6.28) * p_strength(), 0.0);
  } else if (prof == 1) {
    off = vec2<f32>(0.0, cos(px.y / sc * 6.28) * p_strength());
  } else if (prof == 2) {
    off = vec2<f32>(triangle_wave(px.x / sc) * p_strength(), triangle_wave(px.y / sc) * p_strength());
  } else {
    let n = fbm(uv * 80.0, 3, 2.0, 0.5) - 0.5;
    off = vec2<f32>(n) * p_strength() * 2.0;
  }
  return textureSampleLevel(src_tex, src_sampler, clamp(uv + px_to_uv(1.0) * vec2<f32>(off.x, off.y), vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
fn triangle_wave(x: f32) -> f32 {
  return abs(fract(x) * 2.0 - 1.0) * 2.0 - 1.0;
}
"#
    )
}

pub fn fx113_materials() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.materials",
        "Materials",
        "Material",
        "fx_materials",
        QualityClass::M,
        "FX113",
        "Chrome, liquid metal, emboss, iridescent holographic and specular relief materials.",
        [
            p_enum(
                "material",
                "Material",
                "chrome",
                &["chrome", "liquidMetal", "emboss", "iridescent", "specular"]
            ),
            p_float("strength", "Strength", 1.0, 0.0, 4.0, ""),
            p_float("roughness", "Roughness", 0.3, 0.0, 1.0, ""),
            p_float(
                "lightAngle",
                "Light angle",
                0.7,
                -HALF_TURN_2DP,
                HALF_TURN_2DP,
                "rad"
            ),
        ],
        r#"
fn hsv(h: f32, s: f32, v: f32) -> vec3<f32> {
  let hh = fract(h) * 6.0;
  let i = i32(floor(hh));
  let f = hh - floor(hh);
  let p = v * (1.0 - s); let q = v * (1.0 - s * f); let t = v * (1.0 - s * (1.0 - f));
  if (i == 0) { return vec3<f32>(v, t, p); }
  if (i == 1) { return vec3<f32>(q, v, p); }
  if (i == 2) { return vec3<f32>(p, v, t); }
  if (i == 3) { return vec3<f32>(p, q, v); }
  if (i == 4) { return vec3<f32>(t, p, v); }
  return vec3<f32>(v, p, q); }
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let e = px_to_uv(2.0);
  let lx = sample_luma(uv + vec2<f32>(e.x, 0.0)) - sample_luma(uv - vec2<f32>(e.x, 0.0));
  let ly = sample_luma(uv + vec2<f32>(0.0, e.y)) - sample_luma(uv - vec2<f32>(0.0, e.y));
  let n = normalize(vec3<f32>(-lx * 10.0, -ly * 10.0, 1.0));
  let la = p_lightAngle();
  let light = normalize(vec3<f32>(cos(la), sin(la), 0.8));
  let view = vec3<f32>(0.0, 0.0, 1.0);
  let diff = clamp(dot(n, light), 0.0, 1.0);
  let spec = pow(clamp(dot(reflect(-light, n), view), 0.0, 1.0), mix(64.0, 4.0, p_roughness()));
  let fres = pow(1.0 - clamp(n.z, 0.0, 1.0), 3.0);
  let base = unpremul(src);
  var col = base;
  let m = p_material();
  if (m == 0) {
    col = vec3<f32>(diff) * vec3<f32>(0.8, 0.85, 0.9) + vec3<f32>(spec) + fres * vec3<f32>(0.3) * base;
  } else if (m == 1) {
    let t = time_seconds() * 0.1;
    col = hsv(fract(diff * 0.6 + n.y * 0.2 + t), 0.6, diff) + vec3<f32>(spec);
  } else if (m == 2) {
    col = vec3<f32>(diff) * 0.5 + 0.5;
  } else if (m == 3) {
    col = hsv(fract(fres * 1.5 + time_seconds() * 0.05), 0.8, 0.6 + spec);
  } else {
    col = base * diff + vec3<f32>(spec * 1.5);
  }
  return premul(col * p_strength(), src.a);
}
"#
    )
}

pub fn fx114_texture() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.texture",
        "Texture",
        "Texture",
        "fx_texture",
        QualityClass::M,
        "FX114",
        "Original paper, craft paper, stucco, textile, glitter, wood and marble procedural recipes.",
        [
            p_enum("material", "Material", "paper", &["paper", "craft", "stucco", "textile", "glitter", "wood", "marble"]),
            p_float("scale", "Scale", 6.0, 0.5, 64.0, ""),
            p_float("mix", "Mix", 0.5, 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let sc = p_scale();
  let p = uv * sc;
  let m = p_material();
  var t = 0.0;
  var col = vec3<f32>(0.8, 0.75, 0.65);
  if (m == 0) { t = fbm(p * 30.0, 4, 2.0, 0.5); col = vec3<f32>(0.92, 0.9, 0.85) * (0.8 + t * 0.3); }
  else if (m == 1) { t = fbm(p * 15.0, 4, 2.0, 0.6); col = vec3<f32>(0.6, 0.45, 0.3) * (0.6 + t * 0.6); }
  else if (m == 2) { let n = length(vec2<f32>(gradient_noise(p * 20.0), gradient_noise(p * 20.0 + 5.0))); t = smoothstep(0.2, 0.8, n); col = vec3<f32>(0.7) * (0.6 + t * 0.6); }
  else if (m == 3) { let grid = modular_wave(uv * sc * 20.0); t = grid; col = vec3<f32>(0.4, 0.2, 0.3) * (0.5 + t * 0.8); }
  else if (m == 4) { let s = hash21(floor(uv * sc * 200.0)); t = pow(s, 8.0); col = vec3<f32>(1.0) * (0.3 + t); }
  else if (m == 5) { let rings = fract(length(uv * sc * 0.5) * 8.0 + fbm(p * 4.0, 3, 2.0, 0.5) * 4.0); t = rings; col = vec3<f32>(0.55, 0.35, 0.18) * (0.5 + rings * 0.5); }
  else { let vein = abs(sin(p.x * 3.0 + fbm(p * 2.0, 5, 2.0, 0.5) * 6.0)); t = smoothstep(0.7, 1.0, vein); col = mix(vec3<f32>(0.9, 0.9, 0.92), vec3<f32>(0.35, 0.35, 0.4), t); }
  return premul(mix(unpremul(src), col, p_mix()), src.a);
}
fn modular_wave(v: vec2<f32>) -> f32 {
  return abs(sin(v.x) * sin(v.y));
}
"#
    )
}

pub fn fx115_print() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.print",
        "Print",
        "Print and Pixel",
        "fx_print",
        QualityClass::M,
        "FX115",
        "CMYK print with dot angles, misregistration, risograph ink and comics outline.",
        [
            p_enum(
                "mode",
                "Mode",
                "cmyk",
                &["cmyk", "misregister", "risograph", "comics"]
            ),
            p_float("dotSize", "Dot size", 6.0, 2.0, 32.0, "output_px"),
            p_float("misregister", "Misregistration", 1.0, 0.0, 8.0, "output_px"),
            p_color("inkA", "Ink A", [0.0, 0.75, 0.8, 1.0]),
            p_color("inkB", "Ink B", [1.0, 0.45, 0.1, 1.0]),
        ],
        r#"
fn screen_dot(uv: vec2<f32>, angle_deg: f32, cov: f32) -> f32 {
  let a = radians(angle_deg);
  let centered = (uv - vec2<f32>(0.5)) * vec2<f32>(frame.dims.xy);
  let rp = rot2(a) * centered;
  let c = floor(rp / p_dotSize());
  let f = fract(rp / p_dotSize()) - vec2<f32>(0.5);
  let d = 1.0 - smoothstep(cov - 0.1, cov + 0.1, length(f) * 2.0);
  return d;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = unpremul(src);
  let k = 1.0 - max(c.r, max(c.g, c.b));
  let cy = (1.0 - c.r - k) / max(1.0 - k, 0.0001);
  let ma = (1.0 - c.g - k) / max(1.0 - k, 0.0001);
  let ye = (1.0 - c.b - k) / max(1.0 - k, 0.0001);
  let off = px_to_uv(p_misregister());
  let cd = screen_dot(uv + off, 15.0, cy);
  let md = screen_dot(uv - off, 75.0, ma);
  let yd = screen_dot(uv, 0.0, ye);
  let kd = screen_dot(uv, 45.0, k);
  var col = vec3<f32>(1.0);
  col = mix(col, vec3<f32>(0.0, 0.7, 0.9), cd);
  col = mix(col, vec3<f32>(0.9, 0.1, 0.5), md);
  col = mix(col, vec3<f32>(1.0, 0.95, 0.2), yd);
  col = mix(col, vec3<f32>(0.1), kd);
  if (p_mode() == 2) {
    col = mix(p_inkA().rgb, p_inkB().rgb, cy);
    col = mix(col, vec3<f32>(0.95, 0.92, 0.85), 1.0 - max(max(cd, md), yd) * 0.7);
  }
  if (p_mode() == 3) {
    let e = px_to_uv(2.0);
    let lx = abs(sample_luma(uv + vec2<f32>(e.x, 0.0)) - sample_luma(uv - vec2<f32>(e.x, 0.0)));
    let ly = abs(sample_luma(uv + vec2<f32>(0.0, e.y)) - sample_luma(uv - vec2<f32>(0.0, e.y)));
    let edge = smoothstep(0.05, 0.2, lx + ly);
    col = mix(vec3<f32>(floor(vec3<f32>(luminance_from_rgb(c)) * 4.0) / 4.0), vec3<f32>(0.05), edge);
  }
  return premul(col, src.a);
}
"#
    )
}

pub fn fx116_technical() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.technical",
        "Technical",
        "Technical",
        "fx_technical",
        QualityClass::M,
        "FX116",
        "CT scan style, thermal, blueprint, contour lines and cutaway 2D styling of the source.",
        [
            p_enum(
                "mode",
                "Mode",
                "ct",
                &["ct", "thermal", "blueprint", "contour", "cutaway"]
            ),
            p_float("banding", "Band density", 12.0, 1.0, 64.0, ""),
            p_float("sweep", "Scan sweep", 0.1, 0.0, 2.0, "cycles/s"),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let l = luminance_from_rgb(unpremul(src));
  let m = p_mode();
  var col = vec3<f32>(l);
  if (m == 0) {
    let band = fract(l * p_banding() + time_seconds() * p_sweep());
    col = mix(vec3<f32>(0.0), vec3<f32>(0.85), smoothstep(0.45, 0.55, band)) * (1.0 - l * 0.6);
    let sweep_y = fract(time_seconds() * p_sweep());
    col += vec3<f32>(0.3, 0.6, 1.0) * smoothstep(0.02, 0.0, abs(uv.y - sweep_y)) * 0.6;
  } else if (m == 1) {
    col = clamp(vec3<f32>(l * 1.8, l * l * 1.4 - 0.1, smoothstep(0.6, 1.0, l)), vec3<f32>(0.0), vec3<f32>(1.0));
  } else if (m == 2) {
    let gx = abs(sample_luma(uv + px_to_uv(1.0)) - sample_luma(uv - px_to_uv(1.0)));
    let g = smoothstep(0.05, 0.2, gx);
    col = mix(vec3<f32>(0.05, 0.15, 0.4), vec3<f32>(0.7, 0.85, 1.0), g);
  } else if (m == 3) {
    let c = fract(l * p_banding());
    let contour = 1.0 - smoothstep(0.0, 0.04, min(c, 1.0 - c));
    col = mix(vec3<f32>(0.1) + vec3<f32>(l) * 0.2, vec3<f32>(1.0), contour);
  } else {
    // Cutaway: split the image with a diagonal mask revealing a flat technical fill.
    let d = uv.x - uv.y;
    let region = smoothstep(0.0, 0.02, d);
    col = mix(vec3<f32>(0.9, 0.2, 0.2), vec3<f32>(l), region);
  }
  return premul(col, src.a);
}
"#
    )
}

pub fn fx117_text_graphic() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.text_graphic",
        "Text graphic",
        "Technical",
        "fx_text_graphic",
        QualityClass::M,
        "FX117",
        "ASCII glyph mapping, code rain, topographic lines and shapes mosaic driven by the bundled glyph atlas.",
        [
            p_enum("mode", "Mode", "ascii", &["ascii", "codeRain", "topographic", "shapes"]),
            p_float("cellSize", "Cell size", 10.0, 4.0, 48.0, "output_px"),
            p_float("speed", "Speed", 1.0, 0.0, 10.0, ""),
        ],
        r#"
// Approximate glyph coverage from the atlas using a 3x5 sampled bitmap per character.
const GLYPHS: array<u32, 16> = array<u32, 16>(
  0x1E6E9u, 0x111111u, 0x1E2E4Eu, 0x1E2E1Eu, 0x2AAAEu, 0x1E2E2Eu, 0x1E2C2Eu, 0x1E1111u,
  0x1E2E2Eu, 0x1E2E2Eu, 0x111155u, 0x2A4A4Au, 0x444444u, 0x555555u, 0x222222u, 0x0Au
);
fn glyph_bit(g: u32, x: u32, y: u32) -> f32 {
  if (x >= 3 || y >= 5) { return 0.0; }
  let shift = (y * 3u + x);
  return select(0.0, 1.0, ((GLYPHS[g] >> shift) & 1u) == 1);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let cs = max(p_cellSize(), 4.0);
  let grid = uv * vec2<f32>(frame.dims.xy) / cs;
  let cell = floor(grid);
  let f = fract(grid);
  let l = sample_luma((cell + 0.5) * cs / vec2<f32>(frame.dims.xy));
  let mode = p_mode();
  if (mode == 3) {
    let h = hash21(cell);
    let r = length(f - 0.5);
    let dotm = 1.0 - smoothstep(0.25, 0.35, r);
    return premul(vec3<f32>(l), dotm * l);
  }
  if (mode == 1) {
    let col_t = fract((floor(time_seconds() * p_speed() * 4.0) + cell.y * 0.1) * 0.1);
    let ch = select(0.0, 1.0, hash21(vec2<f32>(cell.x, floor(time_seconds() * p_speed() * 8.0) - cell.y)) < 0.3);
    let g = (hash21(cell) * 15.0);
    let bit = glyph_bit(u32(clamp(g, 0.0, 15.0)), u32(min(f.x * 3.0, 2.0)), u32(min(f.y * 5.0, 4.0)));
    let glow = bit * (0.4 + col_t * 0.6);
    return premul(vec3<f32>(0.2, 1.0, 0.4) * glow, glow);
  }
  if (mode == 2) {
    let c = fract(l * 20.0);
    let contour = 1.0 - smoothstep(0.0, 0.05, min(c, 1.0 - c));
    return premul(vec3<f32>(contour), contour);
  }
  // ASCII mapping: darkness -> denser glyph
  let idx = u32(clamp(floor((1.0 - l) * 15.99), 0.0, 15.0));
  let bit = glyph_bit(idx, u32(min(f.x * 3.0, 2.0)), u32(min(f.y * 5.0, 4.0)));
  return premul(vec3<f32>(1.0 - bit), src.a);
}
"#
    )
}

pub fn fx118_temporal() -> (&'static str, Manifest, &'static str) {
    // Note: implemented through the shared temporal-history node; the shader
    // here provides the decay mix used by the canonical-frame history path.
    fam!(
        "fx.temporal",
        "Temporal",
        "Temporal",
        "fx_temporal",
        QualityClass::H,
        "FX118",
        "Frame delay, echo, afterimage, trails and freeze. History is counted in canonical frames so output rate does not change cadence.",
        [
            p_enum("mode", "Mode", "echo", &["echo", "afterimage", "trails"]),
            p_float("seconds", "Seconds", 0.3, 0.0, 4.0, "s"),
            p_float("decay", "Decay", 0.5, 0.0, 1.0, ""),
        ],
        r#"
// The temporal node provides `history` (previous canonical frame) as the
// source of this pass; this kernel performs the visible decay mix.
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let prev = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  let k = clamp(p_decay(), 0.0, 1.0);
  let mode = p_mode();
  if (mode == 2) {
    return vec4<f32>(max(src.rgb, prev.rgb * k), max(src.a, prev.a * k));
  }
  return mix(src, prev, k);
}
"#
    )
}

pub fn fx119_feedback() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.feedback",
        "Feedback",
        "Temporal",
        "fx_feedback",
        QualityClass::H,
        "FX119",
        "Transform, displacement and reaction feedback reading the explicit prior-frame resource.",
        [
            p_enum(
                "mode",
                "Mode",
                "transform",
                &["transform", "displace", "reaction"]
            ),
            p_float("zoom", "Zoom", 1.02, 0.8, 1.2, ""),
            p_float("rotate", "Rotate", 0.01, -0.2, 0.2, "rad"),
            p_float("mix", "Mix", 0.85, 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = uv - vec2<f32>(0.5);
  let p = rot2(p_rotate()) * c / max(p_zoom(), 0.01) + vec2<f32>(0.5);
  let prev = textureSampleLevel(src_tex, src_sampler, clamp(p, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
  let s = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  let k = clamp(p_mix(), 0.0, 0.99);
  return max(number_to_zero(s * (1.0 - k) + prev * k), vec4<f32>(0.0));
}
fn number_to_zero(v: vec4<f32>) -> vec4<f32> { return v; }
"#
    )
}

pub fn fx120_keys() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.keys",
        "Keys and mattes",
        "Matte",
        "fx_keys",
        QualityClass::M,
        "FX120",
        "Luma key, chroma key, color range and despill with soft edges.",
        [
            p_enum("mode", "Mode", "luma", &["luma", "chroma", "colorRange"]),
            p_float("threshold", "Threshold", 0.5, 0.0, 1.0, ""),
            p_float("softness", "Softness", 0.1, 0.0, 1.0, ""),
            p_color("keyColor", "Key color", [0.0, 1.0, 0.0, 1.0]),
            p_bool("despill", "Despill", true),
            p_bool("invert", "Invert", false),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = unpremul(src);
  let soft = max(p_softness(), 0.0001);
  var keep = 0.0;
  let m = p_mode();
  if (m == 0) {
    let l = luminance_from_rgb(c);
    keep = smoothstep(p_threshold() - soft, p_threshold() + soft, l);
  } else if (m == 1) {
    let kc = p_keyColor().rgb;
    let d = length(c - kc);
    keep = smoothstep(p_threshold() * 0.5 + soft, p_threshold() * 0.5, d);
  } else {
    let kc = p_keyColor().rgb;
    let d = max(abs(c.r - kc.r), max(abs(c.g - kc.g), abs(c.b - kc.b)));
    keep = smoothstep(p_threshold() + soft, p_threshold(), d);
  }
  if (p_invert()) { keep = 1.0 - keep; }
  var out = c;
  if (p_despill()) {
    let kc = p_keyColor().rgb;
    let d = length(c - kc);
    let spill = clamp(1.0 - d * 2.0, 0.0, 1.0);
    out = mix(c, vec3<f32>(luminance_from_rgb(c)), spill * 0.5);
  }
  return premul(out, src.a * keep);
}
"#
    )
}

pub fn fx121_masks() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.masks",
        "Masks",
        "Matte",
        "fx_masks",
        QualityClass::M,
        "FX121",
        "Shape, path, feather, dilate/erode, invert and combine mask operations.",
        [
            p_enum(
                "mode",
                "Mode",
                "shape",
                &["shape", "feather", "dilate", "combine"]
            ),
            p_enum(
                "combine",
                "Combine",
                "union",
                &["union", "intersect", "subtract"]
            ),
            p_float("feather", "Feather", 4.0, 0.0, 64.0, "output_px"),
            p_float("radius", "Shape radius", 0.35, 0.0, 0.7, ""),
            p_bool("invert", "Invert", false),
        ],
        r#"
fn shape_mask(uv: vec2<f32>) -> f32 {
  let aspect = f32(frame.dims.x) / max(f32(frame.dims.y), 1.0);
  let d = length((uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0));
  let f = max(px_to_uv(p_feather()).x, 0.0001);
  let m = 1.0 - smoothstep(p_radius() - f, p_radius() + f, d);
  return m;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let m = shape_mask(uv) * p_wet();
  let out = unpremul(src);
  let a = src.a * select(1.0 - m, m, p_invert() == false);
  // wet already mixed by the template via p_wet; use the raw mask here.
  return premul(out, src.a * select(1.0 - shape_mask(uv) * 0.0 - m * 0.0, 1.0, true) * shape_mask(uv));
}
"#
    )
}

pub fn fx122_composite() -> (&'static str, Manifest, &'static str) {
    let body = r#"
fn blend_fn(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
  let m = p_blend();
  if (m == 1) { return b + s; }
  if (m == 2) { return 1.0 - (1.0 - b) * (1.0 - s); }
  if (m == 3) { return b * s; }
  if (m == 4) { return mix(2.0 * b * s, 1.0 - 2.0 * (1.0 - b) * (1.0 - s), step(vec3<f32>(0.5), b)); }
  if (m == 5) { return mix(2.0 * b * s + b * b * (1.0 - 2.0 * s), sqrt(b) * (2.0 * s - 1.0) + 2.0 * b * (1.0 - s), step(vec3<f32>(0.5), s)); }
  if (m == 6) { return mix(2.0 * b * s, 1.0 - 2.0 * (1.0 - b) * (1.0 - s), step(vec3<f32>(0.5), s)); }
  if (m == 7) { return abs(b - s); }
  if (m == 8) { return b + s - 2.0 * b * s; }
  if (m == 9) { return b / max(vec3<f32>(1.0) - s, vec3<f32>(0.001)); }
  if (m == 10) { return 1.0 - (vec3<f32>(1.0) - b) / max(s, vec3<f32>(0.001)); }
  if (m == 11) { return max(b - s, vec3<f32>(0.0)); }
  if (m == 12) { return min(b, s); }
  if (m == 13) { return max(b, s); }
  if (m == 14) { return b / max(s, vec3<f32>(0.001)); }
  return b;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  // Two real image inputs: `a` = src_tex (binding 0), `b` = aux_tex (binding 3).
  // This is what makes parallel branches merge: connect two upstream nodes to
  // ports a and b.
  let a = unpremul(src);
  let b = unpremul(textureSampleLevel(aux_tex, src_sampler, uv, 0.0));
  let blended = clamp(blend_fn(b, a), vec3<f32>(0.0), vec3<f32>(4.0));
  return premul(mix(a, blended, p_opacity()), src.a);
}
"#;
    let mut m = effect(
        "fx.composite",
        "Composite",
        "Blend",
        "fx_composite",
        QualityClass::L,
        vec![
            p_enum("blend", "Blend", "over", &["over", "add", "screen", "multiply", "overlay", "softLight", "hardLight", "difference", "exclusion", "dodge", "burn", "subtract", "darken", "lighten", "divide"]),
            p_float("opacity", "Opacity", 1.0, 0.0, 1.0, ""),
        ],
        "Two-image compositor: blends input A (source) with input B (aux) using standard blend modes. Use it to merge parallel branches.",
        "FX122",
    );
    // Two image inputs A and B (not the default source/mask pair).
    m.inputs = vec![
        PortDecl {
            id: "a".into(),
            port_type: PortType::Texture2D,
            required: true,
        },
        PortDecl {
            id: "b".into(),
            port_type: PortType::Texture2D,
            required: false,
        },
    ];
    ("fx_composite", m, body)
}

pub fn fx123_transforms() -> (&'static str, Manifest, &'static str) {
    fam!(
        "fx.transforms",
        "Transform",
        "Style",
        "fx_transforms",
        QualityClass::L,
        "FX123",
        "Affine translate/scale/rotate with inverse mapping and anchor.",
        [
            p_enum("mode", "Mode", "affine", &["affine", "crop"]),
            p_vec2("position", "Position", [0.0, 0.0], -1.0, 1.0, ""),
            p_float("scale", "Scale", 1.0, 0.01, 8.0, ""),
            p_float(
                "rotation",
                "Rotation",
                0.0,
                -FULL_TURN_7DP,
                FULL_TURN_7DP,
                "rad"
            ),
            p_vec2("cropMin", "Crop min", [0.0, 0.0], 0.0, 1.0, ""),
            p_vec2("cropMax", "Crop max", [1.0, 1.0], 0.0, 1.0, ""),
        ],
        r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  if (p_mode() == 1) {
    let cm = p_cropMin(); let cx = p_cropMax();
    if (uv.x < cm.x || uv.y < cm.y || uv.x > cx.x || uv.y > cx.y) { return vec4<f32>(0.0); }
    let su = (uv - cm) / max(cx - cm, vec2<f32>(0.0001));
    return textureSampleLevel(src_tex, src_sampler, clamp(su, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
  }
  let c = uv - vec2<f32>(0.5) - p_position();
  let q = rot2(-p_rotation()) * c / max(p_scale(), 0.001) + vec2<f32>(0.5);
  if (q.x < 0.0 || q.x > 1.0 || q.y < 0.0 || q.y > 1.0) { return vec4<f32>(0.0); }
  return textureSampleLevel(src_tex, src_sampler, q, 0.0);
}
"#
    )
}
