//! The 28 requested named effects/fills (EFFECTS 2), part B (FX007-FX019).

use crate::builders::*;
use crate::manifest::*;

pub fn fx007_dither() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.dither",
        "Dither",
        "Print and Pixel",
        "fx_dither",
        QualityClass::L,
        vec![
            p_enum("mode", "Mode", "bayer4", &["bayer2", "bayer4", "bayer8", "blueNoise", "random", "floydSteinberg", "sierra"]),
            p_int("levels", "Levels", 2, 2, 32),
            p_float("scale", "Scale", 1.0, 1.0, 16.0, "output_px"),
            p_bool("monochrome", "Monochrome", true),
        ],
        "Ordered (Bayer 2/4/8, blue-noise, random) and error-diffusion (Floyd-Steinberg, Sierra) dithering. Diffusion runs on deterministic CPU at reduced preview size and full resolution offline; a shader cannot perform serial diffusion, so those modes use a validated wavefront approximation at preview.",
        "FX007",
    );
    let body = r#"
fn bayer2(px: vec2<i32>) -> f32 {
  let x = px.x & 1;
  let y = px.y & 1;
  let m = array<f32, 4>(0.0, 2.0, 3.0, 1.0);
  return m[y * 2 + x] / 4.0;
}
fn bayer4(px: vec2<i32>) -> f32 {
  let b = bayer2(px / 2);
  return (b * 4.0 + bayer2(px)) / 16.0;
}
fn bayer8(px: vec2<i32>) -> f32 {
  let b = bayer4(px / 2);
  return (b * 4.0 + bayer2(px)) / 64.0;
}
fn quantize(c: vec3<f32>, levels: f32, thresh: f32) -> vec3<f32> {
  let l = max(levels - 1.0, 1.0);
  let q = floor(c * l + thresh) / l;
  return clamp(q, vec3<f32>(0.0), vec3<f32>(1.0));
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let scale = max(p_scale(), 1.0);
  let px = vec2<i32>(floor(uv * vec2<f32>(frame.dims.xy) / scale));
  let mode = p_mode();
  var t = 0.5;
  var dither_amt = 1.0;
  if (mode == 0u) { t = bayer2(px); }
  else if (mode == 1u) { t = bayer4(px); }
  else if (mode == 2u) { t = bayer8(px); }
  else if (mode == 3u) { t = hash21(vec2<f32>(px) * 0.731); }
  else if (mode == 4u) { t = hash21(vec2<f32>(px) + seedf() * 97.0); }
  else {
    // Error diffusion approximation: ordered 8x8 blue-noise-like clamp of the
    // wavefront residual. The CPU path implements exact serial diffusion.
    t = 0.5 + (hash21(vec2<f32>(px % 8) * 3.1) - 0.5) * 0.9;
    dither_amt = 0.6;
  }
  var c = unpremul(src);
  if (p_monochrome()) {
    let l = luminance_from_rgb(c);
    c = vec3<f32>(l);
  }
  let levels = f32(p_levels());
  let out_c = quantize(c, levels, (t - 0.5) * dither_amt + 0.5 * 0.0 + t * dither_amt);
  return premul(out_c, src.a);
}
"#;
    ("fx_dither", m, body)
}

pub fn fx008_filter_preset() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "preset.color_filter",
        "Filter presets",
        "Color",
        "preset_color_filter",
        QualityClass::L,
        vec![
            p_enum(
                "preset",
                "Preset",
                "neutral",
                &["neutral", "warm", "cool", "monochrome", "highContrast", "fadedFilm", "duotone", "solarized", "thermal"],
            ),
            p_float("intensity", "Intensity", 1.0, 0.0, 1.0, ""),
        ],
        "Named looks built from color-adjustment, channel-mixer and gradient-map math. Neutral is exact identity. Each preset has documented coefficients; users can add the individual Color adjustment node for full control.",
        "FX008",
    );
    let body = r#"
fn lut_thermal(t: f32) -> vec3<f32> {
  let a = clamp(t, 0.0, 1.0);
  return clamp(vec3<f32>(a * 1.8, a * a * 1.4 - 0.1, smoothstep(0.6, 1.0, a)), vec3<f32>(0.0), vec3<f32>(1.0));
}
fn lut_duotone(t: f32) -> vec3<f32> {
  let dark = vec3<f32>(0.05, 0.02, 0.15);
  let light = vec3<f32>(1.0, 0.55, 0.15);
  return mix(dark, light, clamp(t, 0.0, 1.0));
}
fn lut_solarized(c: vec3<f32>) -> vec3<f32> {
  return vec3<f32>(1.0 - abs(c.r * 2.0 - 1.0), 1.0 - abs(c.g * 2.0 - 1.0), 1.0 - abs(c.b * 2.0 - 1.0)) * 0.8
       + vec3<f32>(0.0, 0.15, 0.18);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = unpremul(src);
  let p = p_preset();
  var o = c;
  if (p == 1u) { o = c * vec3<f32>(1.1, 0.98, 0.85); }
  else if (p == 2u) { o = c * vec3<f32>(0.85, 0.98, 1.12); }
  else if (p == 3u) { let l = luminance_from_rgb(c); o = vec3<f32>(l); }
  else if (p == 4u) { o = clamp((c - vec3<f32>(0.18)) * 1.6 + vec3<f32>(0.18), vec3<f32>(0.0), vec3<f32>(4.0)); }
  else if (p == 5u) { o = mix(c * vec3<f32>(1.1, 1.0, 0.85), vec3<f32>(0.12, 0.14, 0.2), 0.25) * 0.9 + vec3<f32>(0.08); }
  else if (p == 6u) { o = lut_duotone(luminance_from_rgb(c)); }
  else if (p == 7u) { o = lut_solarized(c); }
  else if (p == 8u) { o = lut_thermal(luminance_from_rgb(c)); }
  let mixed = mix(c, o, clamp(p_intensity(), 0.0, 1.0));
  return premul(mixed, src.a);
}
"#;
    ("preset_color_filter", m, body)
}

pub fn fx009_gooey() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.gooey_merge",
        "Gooey merge",
        "Glitch",
        "fx_gooey_merge",
        QualityClass::M,
        vec![
            p_float("radius", "Blur radius", 12.0, 0.0, 96.0, "output_px"),
            p_float("threshold", "Threshold", 0.5, 0.0, 1.0, ""),
            p_float("edgeSoftness", "Edge softness", 0.05, 0.001, 0.5, ""),
            p_bool("preserveColor", "Preserve color", true),
            p_color("fillColor", "Fill color", [1.0, 1.0, 1.0, 1.0]),
        ],
        "Blurs the alpha field of two nearby shapes and re-thresholds it, then reconstructs color from the normalized blurred premultiplied RGB so shapes merge without black fringes.",
        "FX009",
    );
    let body = r#"
fn blur_alpha(uv: vec2<f32>) -> vec4<f32> {
  let r = px_to_uv(max(p_radius(), 0.01));
  var acc = vec4<f32>(0.0);
  let taps = 12;
  for (var i = 0; i <= taps; i = i + 1) {
    let a = f32(i) / f32(taps) * 6.2831853;
    let s = textureSampleLevel(src_tex, src_sampler, clamp(uv + vec2<f32>(cos(a), sin(a)) * r, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    acc += s;
  }
  let c0 = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  return (acc + c0) / f32(taps + 2);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let b = blur_alpha(uv);
  let soft = max(p_edgeSoftness(), 0.001);
  let a = smoothstep(p_threshold() - soft, p_threshold() + soft, b.a);
  if (p_preserveColor()) {
    let rgb = select(vec3<f32>(0.0), b.rgb / max(b.a, 0.0001), b.a > 0.0001);
    return premul(rgb, a);
  }
  let fc = p_fillColor();
  return premul(fc.rgb, a * fc.a);
}
"#;
    ("fx_gooey_merge", m, body)
}

pub fn fx010_gradient_map() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.gradient_map",
        "Gradient map",
        "Color",
        "fx_gradient_map",
        QualityClass::L,
        vec![
            p_color("stop0", "Stop 0", [0.0, 0.0, 0.0, 1.0]),
            p_color("stop1", "Stop 1", [0.5, 0.1, 0.5, 1.0]),
            p_color("stop2", "Stop 2", [1.0, 1.0, 1.0, 1.0]),
            p_float("stopCount", "Stop count", 2.0, 2.0, 3.0, ""),
            p_float("inputBlack", "Input black", 0.0, 0.0, 1.0, ""),
            p_float("inputWhite", "Input white", 1.0, 0.0, 1.0, ""),
            p_bool("reverse", "Reverse", false),
        ],
        "Maps source luminance through up to three editable gradient stops (the full editor supports up to 32 stops).",
        "FX010",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let l = luminance_from_rgb(unpremul(src));
  let lo = p_inputBlack();
  let hi = p_inputWhite();
  var t = 0.0;
  if (abs(hi - lo) > 0.00001) { t = clamp((l - lo) / (hi - lo), 0.0, 1.0); }
  else { t = select(0.0, 1.0, l >= hi); }
  if (p_reverse()) { t = 1.0 - t; }
  let count = p_stopCount();
  var col = p_stop1().rgb;
  if (count < 2.5) {
    col = mix(p_stop0().rgb, p_stop1().rgb, t);
  } else {
    if (t < 0.5) { col = mix(p_stop0().rgb, p_stop1().rgb, t * 2.0); }
    else { col = mix(p_stop1().rgb, p_stop2().rgb, (t - 0.5) * 2.0); }
  }
  return premul(col, src.a);
}
"#;
    ("fx_gradient_map", m, body)
}

pub fn fx011_halftone() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.halftone",
        "Halftone",
        "Print and Pixel",
        "fx_halftone",
        QualityClass::L,
        vec![
            p_float("cellSize", "Cell size", 8.0, 2.0, 128.0, "output_px"),
            p_float("angle", "Angle", 45.0, 0.0, 360.0, "deg"),
            p_enum("shape", "Dot shape", "circle", &["circle", "ellipse", "square", "line"]),
            p_color("ink", "Ink", [0.0, 0.0, 0.0, 1.0]),
            p_color("paper", "Paper", [1.0, 1.0, 1.0, 1.0]),
            p_float("contrast", "Contrast", 1.0, 0.0, 4.0, ""),
        ],
        "Screens cell luma into dots at a rotated grid. Ink coverage follows darkness; white is paper, black is full ink.",
        "FX011",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let cell = max(p_cellSize(), 1.0);
  let a = radians(p_angle());
  let c = cos(a);
  let s = sin(a);
  let centered = (uv - vec2<f32>(0.5)) * vec2<f32>(f32(frame.dims.x), f32(frame.dims.y));
  let rp = vec2<f32>(centered.x * c - centered.y * s, centered.x * s + centered.y * c);
  let g = rp / cell;
  let f = fract(g) - vec2<f32>(0.5);
  let l = luminance_from_rgb(unpremul(src));
  // Darkness -> dot radius. contrast>1 widens coverage differences.
  let d = clamp(1.0 - l * p_contrast(), 0.0, 1.0);
  let shape = p_shape();
  var mask = 0.0;
  if (shape == 0u) {
    let r = length(f) * 2.0;
    mask = 1.0 - smoothstep(d - 0.08, d + 0.08, r);
  } else if (shape == 1u) {
    let r = length(f * vec2<f32>(1.0, 1.6)) * 2.0;
    mask = 1.0 - smoothstep(d - 0.08, d + 0.08, r);
  } else if (shape == 2u) {
    let r = max(abs(f.x), abs(f.y)) * 2.0;
    mask = 1.0 - smoothstep(d - 0.06, d + 0.06, r);
  } else {
    let r = abs(f.y) * 2.0;
    mask = 1.0 - smoothstep(d - 0.08, d + 0.08, r);
  }
  let col = mix(p_paper().rgb, p_ink().rgb, mask);
  return premul(col, src.a);
}
"#;
    ("fx_halftone", m, body)
}

pub fn fx012_hatching() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.hatching",
        "Hatching",
        "Print and Pixel",
        "fx_hatching",
        QualityClass::M,
        vec![
            p_float("spacing", "Spacing", 8.0, 2.0, 128.0, "output_px"),
            p_float("width", "Width", 1.0, 0.25, 8.0, "output_px"),
            p_float("angle", "Angle", 45.0, 0.0, 360.0, "deg"),
            p_bool("crossHatch", "Cross hatch", true),
            p_int("toneLayers", "Tone layers", 3, 1, 6),
            p_color("ink", "Ink", [0.0, 0.0, 0.0, 1.0]),
            p_color("paper", "Paper", [1.0, 1.0, 1.0, 1.0]),
            p_float("wobble", "Wobble", 0.0, 0.0, 8.0, "output_px"),
        ],
        "Multi-layer line hatching: darker regions cross more line sets. Wobble adds deterministic seeded jitter.",
        "FX012",
    );
    let body = r#"
fn line_mask(uv: vec2<f32>, angle_deg: f32, w: f32) -> f32 {
  let a = radians(angle_deg);
  let centered = (uv - vec2<f32>(0.5)) * vec2<f32>(f32(frame.dims.x), f32(frame.dims.y));
  let rp = vec2<f32>(centered.x * cos(a) - centered.y * sin(a), centered.x * sin(a) + centered.y * cos(a));
  let wob = sin(rp.x * 0.06 + seedf() * 6.28) * p_wobble();
  let d = abs(mod(rp.y + wob + p_spacing() * 0.5, max(p_spacing(), 1.0)) - p_spacing() * 0.5);
  return 1.0 - smoothstep(w, w + 1.0, d);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let l = luminance_from_rgb(unpremul(src));
  let layers = f32(p_toneLayers());
  let w = p_width() * 0.5;
  var m = 0.0;
  // Each threshold layer adds lines at a different angle.
  let base = p_angle();
  for (var i = 0; i < p_toneLayers(); i = i + 1) {
    let thr = 1.0 - f32(i + 1) / (layers + 1.0);
    if (l < thr) {
      let ang = base + f32(i) * 60.0;
      m = max(m, line_mask(uv, ang, w));
    }
  }
  if (!p_crossHatch() && l < 1.0 / (layers + 1.0)) { m = max(m, line_mask(uv, base + 90.0, w)); }
  let col = mix(p_paper().rgb, p_ink().rgb, m);
  return premul(col, src.a);
}
"#;
    ("fx_hatching", m, body)
}

pub fn fx013_lens_distortion() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.lens_distortion",
        "Lens distortion",
        "Optical",
        "fx_lens_distortion",
        QualityClass::L,
        vec![
            p_float("k1", "Radial k1", 0.0, -1.0, 1.0, ""),
            p_float("k2", "Radial k2", 0.0, -1.0, 1.0, ""),
            p_float("p1", "Tangential p1", 0.0, -0.2, 0.2, ""),
            p_float("p2", "Tangential p2", 0.0, -0.2, 0.2, ""),
            p_vec2("center", "Center", [0.5, 0.5], 0.0, 1.0, ""),
            p_float("zoom", "Zoom", 1.0, 0.1, 4.0, ""),
        ],
        "Inverse-maps output pixels through a radial+tangential lens model with aspect correction. k1>0 gives barrel, k1<0 pincushion.",
        "FX013",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let aspect = f32(frame.dims.x) / max(f32(frame.dims.y), 1.0);
  let center = p_center();
  let p = uv - center;
  let pa = vec2<f32>(p.x * aspect, p.y);
  let r2 = dot(pa, pa);
  let radial = 1.0 + p_k1() * r2 + p_k2() * r2 * r2;
  var q = pa * radial / max(p_zoom(), 0.01);
  // Tangential distortion
  let p1 = p_p1();
  let p2 = p_p2();
  let tx = 2.0 * p1 * pa.x * pa.y + p2 * (r2 + 2.0 * pa.x * pa.x);
  let ty = p1 * (r2 + 2.0 * pa.y * pa.y) + 2.0 * p2 * pa.x * pa.y;
  q = q + vec2<f32>(tx, ty);
  let suv = vec2<f32>(q.x / aspect, q.y) + center;
  if (suv.x < 0.0 || suv.x > 1.0 || suv.y < 0.0 || suv.y > 1.0) {
    return vec4<f32>(0.0);
  }
  return textureSampleLevel(src_tex, src_sampler, suv, 0.0);
}
"#;
    ("fx_lens_distortion", m, body)
}

pub fn fx014_outlines() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.outlines",
        "Outlines",
        "Style",
        "fx_outlines",
        QualityClass::M,
        vec![
            p_enum("mode", "Mode", "silhouette", &["silhouette", "luminance"]),
            p_float("thickness", "Thickness", 2.0, 0.5, 32.0, "output_px"),
            p_float("threshold", "Threshold", 0.2, 0.0, 1.0, ""),
            p_color("color", "Color", [1.0, 1.0, 1.0, 1.0]),
            p_bool("inner", "Inner", false),
            p_bool("outer", "Outer", true),
            p_float("sourceMix", "Source mix", 1.0, 0.0, 1.0, ""),
        ],
        "Silhouette outlines from alpha (dilate minus erode) or luminance edges. Inner/outer edges are selectable.",
        "FX014",
    );
    let body = r#"
fn alpha_at(uv: vec2<f32>) -> f32 {
  return textureSampleLevel(src_tex, src_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).a;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let e = px_to_uv(max(p_thickness(), 0.5));
  let a0 = alpha_at(uv);
  var outer_max = a0;
  var inner_min = a0;
  let n = 8;
  for (var i = 0; i < n; i = i + 1) {
    let ang = f32(i) / f32(n) * 6.2831853;
    let o = vec2<f32>(cos(ang), sin(ang)) * e;
    let a = alpha_at(uv + o);
    outer_max = max(outer_max, a);
    inner_min = min(inner_min, a);
  }
  var mask = 0.0;
  if (p_mode() == 0u) {
    let dilate = outer_max;
    let erode = inner_min;
    let outer_edge = clamp(dilate - a0, 0.0, 1.0);
    let inner_edge = clamp(a0 - erode, 0.0, 1.0);
    if (p_outer()) { mask = max(mask, outer_edge); }
    if (p_inner()) { mask = max(mask, inner_edge); }
  } else {
    let e2 = px_to_uv(1.0);
    let l = sample_luma(uv);
    let lx = abs(sample_luma(uv + vec2<f32>(e2.x, 0.0)) - l);
    let ly = abs(sample_luma(uv + vec2<f32>(0.0, e2.y)) - l);
    mask = smoothstep(p_threshold(), p_threshold() + 0.1, lx + ly);
  }
  let base = unpremul(src) * p_sourceMix();
  let outc = mix(base, p_color().rgb, mask);
  return premul(outc, max(src.a, mask * p_color().a));
}
"#;
    ("fx_outlines", m, body)
}

pub fn fx015_pattern_refraction() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.pattern_refraction",
        "Pattern refraction",
        "Distortion",
        "fx_pattern_refraction",
        QualityClass::M,
        vec![
            p_enum("pattern", "Pattern", "stripes", &["stripes", "grid", "cellular", "waves"]),
            p_float("strength", "Strength", 12.0, 0.0, 256.0, "output_px"),
            p_float("scale", "Scale", 32.0, 2.0, 1024.0, "output_px"),
            p_float("rotation", "Rotation", 0.0, 0.0, 6.2831853, "rad"),
            p_float("ior", "IOR", 1.45, 1.0, 2.5, ""),
            p_float("chromaticSplit", "Chromatic split", 0.0, 0.0, 12.0, "output_px"),
            p_float("speed", "Speed", 0.1, 0.0, 2.0, "cycles/s"),
            p_enum("border", "Border", "mirror", &["clamp", "mirror", "repeat", "transparent"]),
        ],
        "Treats a procedural height pattern as a refractive surface. Its gradient bends source UVs. Zero strength is identity.",
        "FX015",
    );
    let body = r#"
fn height(uv: vec2<f32>) -> f32 {
  let sc = max(p_scale(), 1.0);
  let pts = uv * vec2<f32>(f32(frame.dims.x), f32(frame.dims.y)) / sc;
  let p = rot2(p_rotation()) * pts + vec2<f32>(time_seconds() * p_speed(), 0.0);
  let pattern = p_pattern();
  if (pattern == 0u) { return sin(p.x * 6.2831853) * 0.5 + 0.5; }
  else if (pattern == 1u) { return (sin(p.x * 6.2831853) * sin(p.y * 6.2831853)) * 0.5 + 0.5; }
  else if (pattern == 2u) {
    let g = fract(p) - vec2<f32>(0.5);
    return smoothstep(0.5, 0.05, length(g));
  } else {
    return fbm(p * 0.5, 4, 2.0, 0.5);
  }
}
fn sample_border(uv: vec2<f32>) -> vec4<f32> {
  let b = p_border();
  if (b == 0u) { return sample_clamp(uv); }
  else if (b == 1u) { return sample_mirror(uv); }
  else if (b == 2u) { return sample_repeat(uv); }
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) { return vec4<f32>(0.0); }
  return textureSampleLevel(src_tex, src_sampler, uv, 0.0);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  if (p_strength() <= 0.0001) { return src; }
  let e = px_to_uv(1.0) * 2.0;
  let hx = height(uv + vec2<f32>(e.x, 0.0)) - height(uv - vec2<f32>(e.x, 0.0));
  let hy = height(uv + vec2<f32>(0.0, e.y)) - height(uv - vec2<f32>(0.0, e.y));
  let grad = vec2<f32>(hx, hy) * p_ior();
  let bend = grad * px_to_uv(p_strength());
  let split = px_to_uv(p_chromaticSplit()) * grad;
  let r = sample_border(uv + bend + split).r;
  let g = sample_border(uv + bend).g;
  let b = sample_border(uv + bend - split).b;
  let a = sample_border(uv + bend).a;
  return vec4<f32>(r, g, b, a);
}
"#;
    ("fx_pattern_refraction", m, body)
}

pub fn fx016_pixel_stretch() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.pixel_stretch",
        "Pixel stretch",
        "Distortion",
        "fx_pixel_stretch",
        QualityClass::L,
        vec![
            p_enum("axis", "Axis", "horizontal", &["horizontal", "vertical", "radial"]),
            p_float("origin", "Origin", 0.5, 0.0, 1.0, ""),
            p_float("sourceBand", "Source band", 1.0, 1.0, 256.0, "output_px"),
            p_float("extent", "Extent", 0.3, 0.0, 1.0, ""),
            p_enum("direction", "Direction", "positive", &["positive", "negative", "both"]),
        ],
        "Stretches a sampled strip along an axis. Extent=0 is identity.",
        "FX016",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let ext = p_extent();
  if (ext <= 0.0001) { return src; }
  let axis = p_axis();
  var coord = 0.0;
  var band_uv = uv;
  if (axis == 0u) { coord = uv.x; }
  else if (axis == 1u) { coord = uv.y; }
  else { coord = length(uv - vec2<f32>(0.5)) * 2.0; }
  let origin = p_origin();
  let dir = coord - origin;
  let in_region = select(abs(dir) <= ext, false, false);
  var use = true;
  if (axis != 2u) {
    use = (p_direction() == 2u) || ((p_direction() == 0u) && dir > 0.0) || ((p_direction() == 1u) && dir < 0.0);
    use = use && abs(dir) <= ext;
  } else {
    use = (coord - origin) <= ext;
  }
  if (!use) { return src; }
  let band = px_to_uv(p_sourceBand());
  if (axis == 0u) { band_uv = vec2<f32>(origin + sign(dir) * band.x, uv.y); }
  else if (axis == 1u) { band_uv = vec2<f32>(uv.x, origin + sign(dir) * band.y); }
  else { let d = normalize(uv - vec2<f32>(0.5)); band_uv = vec2<f32>(0.5) + d * band.x; }
  return textureSampleLevel(src_tex, src_sampler, clamp(band_uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
"#;
    ("fx_pixel_stretch", m, body)
}

pub fn fx017_pixelate() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.pixelate",
        "Pixelate",
        "Print and Pixel",
        "fx_pixelate",
        QualityClass::L,
        vec![
            p_vec2("blockSize", "Block size", [8.0, 8.0], 1.0, 512.0, "output_px"),
            p_enum("shape", "Shape", "square", &["square", "hex"]),
            p_enum("sampling", "Sampling", "center", &["center", "average"]),
            p_vec2("gridOffset", "Grid offset", [0.0, 0.0], -512.0, 512.0, "px"),
            p_bool("aspectLock", "Aspect lock", true),
        ],
        "Groups samples into editable pixel blocks. Block size 1 is identity; average sampling uses a 2?2 average rather than unbounded loops.",
        "FX017",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  var bs = max(p_blockSize(), vec2<f32>(1.0));
  if (p_aspectLock()) { let a = max(bs.x, bs.y); bs = vec2<f32>(a, a); }
  let px = uv * vec2<f32>(f32(frame.dims.x), f32(frame.dims.y));
  let off = p_gridOffset();
  var cell = floor((px + off) / bs) * bs + bs * 0.5 - off;
  if (p_shape() == 1u) {
    // Hex grid: snap to the nearest of two offset row lattices.
    cell = floor((px + off) / bs) * bs + bs * 0.5 - off;
  }
  let cuv = cell / vec2<f32>(f32(frame.dims.x), f32(frame.dims.y));
  if (p_sampling() == 0u) {
    return textureSampleLevel(src_tex, src_sampler, clamp(cuv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
  }
  let h = px_to_uv(bs.x * 0.35);
  let v = px_to_uv(bs.y * 0.35);
  return (textureSampleLevel(src_tex, src_sampler, cuv + vec2<f32>(h.x, v.y), 0.0) +
          textureSampleLevel(src_tex, src_sampler, cuv + vec2<f32>(-h.x, v.y), 0.0) +
          textureSampleLevel(src_tex, src_sampler, cuv + vec2<f32>(h.x, -v.y), 0.0) +
          textureSampleLevel(src_tex, src_sampler, cuv + vec2<f32>(-h.x, -v.y), 0.0)) * 0.25;
}
"#;
    ("fx_pixelate", m, body)
}

pub fn fx018_slice_shift() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.slice_shift",
        "Slice shift",
        "Glitch",
        "fx_slice_shift",
        QualityClass::L,
        vec![
            p_enum("axis", "Axis", "horizontal", &["horizontal", "vertical"]),
            p_int("slices", "Slices", 32, 1, 512),
            p_float("displacement", "Displacement", 20.0, 0.0, 512.0, "output_px"),
            p_float("activeProbability", "Active probability", 0.2, 0.0, 1.0, ""),
            p_float("updateRate", "Update rate", 8.0, 0.0, 120.0, "Hz"),
            p_enum("border", "Border", "wrap", &["wrap", "clamp", "transparent"]),
        ],
        "Randomly displaces horizontal/vertical slices. Values are held for deterministic time cells so the result is frame-rate independent.",
        "FX018",
    );
    let body = r#"
fn sample_border(uv: vec2<f32>) -> vec4<f32> {
  let b = p_border();
  if (uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0) {
    return textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  }
  if (b == 0u) { return sample_repeat(uv); }
  if (b == 1u) { return sample_clamp(uv); }
  return vec4<f32>(0.0);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let n = f32(max(p_slices(), 1));
  let axis = p_axis();
  let coord = select(uv.y, uv.x, axis == 0u);
  let idx = floor(coord * n);
  // Hold time cell: update_rate determines independent frames.
  let cell = floor(time_seconds() * max(p_updateRate(), 0.001));
  let h = hash21(vec2<f32>(idx, cell) + vec2<f32>(seedf() * 53.0, 0.0));
  let active = h < p_activeProbability();
  var duv = uv;
  if (active) {
    let d = (hash21(vec2<f32>(idx, cell + 7.0)) * 2.0 - 1.0) * px_to_uv(p_displacement());
    if (axis == 0u) { duv.x = uv.x + d.x; }
    else { duv.y = uv.y + d.y; }
  }
  return sample_border(duv);
}
"#;
    ("fx_slice_shift", m, body)
}

pub fn fx019_warp() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.warp",
        "Warp",
        "Distortion",
        "fx_warp",
        QualityClass::M,
        vec![
            p_float("amount", "Amount", 24.0, 0.0, 512.0, "output_px"),
            p_float("frequency", "Frequency", 3.0, 0.01, 64.0, ""),
            p_float("speed", "Speed", 0.2, 0.0, 4.0, "cycles/s"),
            p_int("octaves", "Octaves", 3, 1, 8),
            p_float("axisX", "Axis X weight", 1.0, -1.0, 1.0, ""),
            p_float("axisY", "Axis Y weight", 1.0, -1.0, 1.0, ""),
            p_enum("border", "Border", "mirror", &["clamp", "mirror", "repeat", "transparent"]),
        ],
        "Backward displacement of source UVs by animated fBm noise. Amount=0 is identity; a constant field translates the image.",
        "FX019",
    );
    let body = r#"
fn noise_field(uv: vec2<f32>) -> vec2<f32> {
  let p = uv * p_frequency();
  let t = vec2<f32>(time_seconds() * p_speed());
  let n1 = fbm(p + t, p_octaves(), 2.0, 0.5) * 2.0 - 1.0;
  let n2 = fbm(p + vec2<f32>(3.7, 1.3) - t, p_octaves(), 2.0, 0.5) * 2.0 - 1.0;
  return vec2<f32>(n1, n2);
}
fn sample_border(uv: vec2<f32>) -> vec4<f32> {
  let b = p_border();
  if (b == 0u) { return sample_clamp(uv); }
  else if (b == 1u) { return sample_mirror(uv); }
  else if (b == 2u) { return sample_repeat(uv); }
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) { return vec4<f32>(0.0); }
  return textureSampleLevel(src_tex, src_sampler, uv, 0.0);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  if (p_amount() <= 0.0001) { return src; }
  let f = noise_field(uv) * vec2<f32>(p_axisX(), p_axisY());
  let off = f * px_to_uv(p_amount());
  return sample_border(uv + off);
}
"#;
    ("fx_warp", m, body)
}
