//! The 28 requested named effects/fills (EFFECTS 2).
//!
//! Each entry has a manifest and a WGSL body implementing `process(uv, src)`.
//! FX020-FX028 are generator sources with transparent backgrounds.

use crate::builders::*;
use crate::manifest::*;
use crate::Catalog;

pub fn fx001_bloom() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.bloom",
        "Bloom",
        "Light",
        "fx_bloom",
        QualityClass::M,
        vec![
            p_float("threshold", "Threshold", 1.0, 0.0, 8.0, "linear"),
            p_float("softKnee", "Soft knee", 0.5, 0.0, 1.0, ""),
            p_float("radius", "Radius", 24.0, 0.0, 256.0, "output_px"),
            p_float("intensity", "Intensity", 0.6, 0.0, 4.0, ""),
            p_float("tintR", "Tint R", 1.0, 0.0, 1.0, ""),
            p_float("tintG", "Tint G", 1.0, 0.0, 1.0, ""),
            p_float("tintB", "Tint B", 1.0, 0.0, 1.0, ""),
            p_bool("expandAlpha", "Expand alpha", false),
        ],
        "Extracts bright linear RGB above a soft-knee threshold, blurs it radially, and adds it back as radiance. Source alpha is preserved unless Expand alpha is on.",
        "FX001",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let thr = min(p_threshold(), 7.99);
  let knee = max(p_softKnee(), 0.0);
  let r = p_radius();
  let tint = vec3<f32>(p_tintR(), p_tintG(), p_tintB());
  // Bright extract with soft knee.
  var acc = vec3<f32>(0.0);
  let center = unpremul(src);
  let bright = max(center - vec3<f32>(thr), vec3<f32>(0.0)) / max(center + vec3<f32>(knee), vec3<f32>(0.001));
  var taps = 0.0;
  let radius_uv = px_to_uv(r);
  for (var ring = 1; ring <= 3; ring = ring + 1) {
    let rn = f32(ring) / 3.0;
    let samples = 8.0 * f32(ring);
    for (var i = 0.0; i < samples; i = i + 1.0) {
      let a = i / samples * 6.2831853;
      let off = vec2<f32>(cos(a), sin(a)) * radius_uv * rn;
      let s = textureSampleLevel(src_tex, src_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
      let c = unpremul(s);
      acc += c * bright * exp(-rn * rn * 2.0);
      taps += 1.0;
    }
  }
  if (taps > 0.0) { acc = acc / taps; }
  let glow = acc * tint * enroll_intensity();
  var out = src;
  let add = glow;
  out = vec4<f32>(src.rgb + add, src.a);
  if (p_expandAlpha()) {
    let g = max(add.r, max(add.g, add.b));
    out.a = max(src.a, min(g, 1.0));
    out.rgb = out.rgb + src.rgb * (1.0 - src.a) * 0.0;
  }
  return out;
}
fn enroll_intensity() -> f32 { return p_intensity(); }
"#;
    ("fx.bloom", m, body)
}

pub fn fx002_bokeh() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.bokeh_blur",
        "Bokeh blur",
        "Optical",
        "fx_bokeh_blur",
        QualityClass::H,
        vec![
            p_float("radius", "Aperture radius", 12.0, 0.0, 96.0, "output_px"),
            p_int("blades", "Blades", 6, 3, 12),
            p_float("rotation", "Rotation", 0.0, 0.0, 6.2831853, "rad"),
            p_float("highlightBoost", "Highlight boost", 0.0, 0.0, 4.0, ""),
            p_enum("quality", "Quality", "32", &["16", "32", "64"]),
        ],
        "Circular or polygonal aperture blur using a deterministic tap distribution. Preview scale is compensated so the aperture matches export.",
        "FX002",
    );
    let body = r#"
fn poly_aperture(theta: f32, blades: f32) -> f32 {
  // Radius of a regular polygon aperture at angle theta (unit circle scaled).
  let n = max(blades, 3.0);
  let seg = 6.2831853 / n;
  let a = mod(theta + p_rotation(), seg);
  let half = seg * 0.5;
  return cos(half) / max(cos(a - half), 0.0001);
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let r = p_radius();
  if (r <= 0.0001) { return src; }
  let taps_f = select(16.0, select(32.0, 64.0, p_quality() == 2u), p_quality() == 1u);
  let taps = i32(taps_f);
  let blades = f32(p_blades());
  let radius_uv = px_to_uv(r);
  var acc = src;
  var norm = 1.0;
  for (var i = 0; i < taps; i = i + 1) {
    let fi = f32(i);
    let t = (fi + 0.5) / taps_f;
    let ang = fi * 2.39996323;
    let rad = sqrt(t);
    let shape = poly_aperture(ang, blades);
    let off = vec2<f32>(cos(ang), sin(ang)) * rad * shape * radius_uv;
    let s = textureSampleLevel(src_tex, src_sampler, clamp(uv + off, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    // Highlight boost: brighter source contributes slightly more.
    let w = 1.0 + luma_lin(unpremul(s)) * p_highlightBoost();
    acc = acc + s * w;
    norm += w;
  }
  return acc / norm;
}
"#;
    ("fx.bokeh_blur", m, body)
}

pub fn fx003_channel_mixer() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.channel_mixer",
        "Channel mixer",
        "Color",
        "fx_channel_mixer",
        QualityClass::L,
        vec![
            p_float("rr", "R->R", 1.0, -2.0, 2.0, ""),
            p_float("rg", "G->R", 0.0, -2.0, 2.0, ""),
            p_float("rb", "B->R", 0.0, -2.0, 2.0, ""),
            p_float("gr", "R->G", 0.0, -2.0, 2.0, ""),
            p_float("gg", "G->G", 1.0, -2.0, 2.0, ""),
            p_float("gb", "B->G", 0.0, -2.0, 2.0, ""),
            p_float("br", "R->B", 0.0, -2.0, 2.0, ""),
            p_float("bg", "G->B", 0.0, -2.0, 2.0, ""),
            p_float("bb", "B->B", 1.0, -2.0, 2.0, ""),
            p_float("offset", "Offset", 0.0, -2.0, 2.0, ""),
            p_bool("monochrome", "Monochrome", false),
        ],
        "Applies a 3?3 RGB matrix and offset to unpremultiplied RGB, preserving alpha.",
        "FX003",
    );
    let body = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = unpremul(src);
  let m = mat3x3<f32>(
    vec3<f32>(p_rr(), p_rg(), p_rb()),
    vec3<f32>(p_gr(), p_gg(), p_gb()),
    vec3<f32>(p_br(), p_bg(), p_bb())
  );
  var out = m * c + vec3<f32>(p_offset());
  if (p_monochrome()) {
    let l = dot(out, vec3<f32>(0.2126, 0.7152, 0.0722));
    out = vec3<f32>(l);
  }
  return premul(out, src.a);
}
"#;
    ("fx.channel_mixer", m, body)
}

pub fn fx004_chromatic_metal() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.chromatic_metal",
        "Chromatic metal",
        "Material",
        "fx_chromatic_metal",
        QualityClass::M,
        vec![
            p_float("emboss", "Emboss", 4.0, 0.0, 32.0, "output_px"),
            p_float("roughness", "Roughness", 0.25, 0.0, 1.0, ""),
            p_float("metallic", "Metallic", 1.0, 0.0, 1.0, ""),
            p_float("hueShift", "Hue shift", 0.0, -1.0, 1.0, ""),
            p_float("lightAngle", "Light angle", 0.7, -3.1415926, 3.1415926, "rad"),
            p_float("environmentSpeed", "Environment speed", 0.1, 0.0, 2.0, "cycles/s"),
        ],
        "Treats source luminance as relief and shades it with a rainbow environment and Fresnel rim. This is a stylized 2D material, not reconstructed 3D geometry.",
        "FX004",
    );
    let body = r#"
fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
  let k = vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0);
  let p = abs(fract(c.xxx + k) * 6.0 - vec3<f32>(3.0));
  return clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)) * (clamp(c.y, 0.0, 1.0) - 1.0) * 0.0 + mix(vec3<f32>(1.0), clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0)), clamp(c.y, 0.0, 1.0));
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let e = px_to_uv(max(p_emboss(), 0.5));
  let l0 = sample_luma(uv);
  let lx = sample_luma(uv + vec2<f32>(e.x, 0.0)) - sample_luma(uv - vec2<f32>(e.x, 0.0));
  let ly = sample_luma(uv + vec2<f32>(0.0, e.y)) - sample_luma(uv - vec2<f32>(0.0, e.y));
  let n = normalize(vec3<f32>(-lx * 8.0, -ly * 8.0, 1.0));
  let l = normalize(vec3<f32>(cos(p_lightAngle()), -sin(p_lightAngle()), 0.7));
  let diff = clamp(dot(n, l), 0.0, 1.0);
  let view = vec3<f32>(0.0, 0.0, 1.0);
  let fres = pow(1.0 - clamp(dot(n, view), 0.0, 1.0), 3.0);
  let t = time_seconds() * p_environmentSpeed();
  let hue = fract(l0 * 0.5 + n.y * 0.15 + fres * 0.3 + t + p_hueShift());
  let env = hsv2rgb(vec3<f32>(hue, 0.8, 1.0));
  let rough = clamp(p_roughness(), 0.0, 1.0);
  let metal = clamp(p_metallic(), 0.0, 1.0);
  let base = unpremul(src);
  let out = mix(base * diff, base * diff + env * metal * (0.4 + fres), metal) * (1.0 - rough * 0.5)
          + env * fres * 0.7;
  return premul(out, src.a);
}
"#;
    ("fx_chromatic_metal", m, body)
}

pub fn fx005_color_adjustment() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.color_adjustment",
        "Color adjustment",
        "Color",
        "fx_color_adjustment",
        QualityClass::L,
        vec![
            p_float("exposure", "Exposure", 0.0, -10.0, 10.0, "EV"),
            p_float("contrast", "Contrast", 1.0, 0.0, 4.0, ""),
            p_float("saturation", "Saturation", 1.0, 0.0, 3.0, ""),
            p_float("hue", "Hue", 0.0, -180.0, 180.0, "deg"),
            p_float("temperature", "Temperature", 0.0, -1.0, 1.0, ""),
            p_float("tint", "Tint", 0.0, -1.0, 1.0, ""),
            p_float("lift", "Lift", 0.0, -1.0, 1.0, ""),
            p_float("gamma", "Gamma", 1.0, 0.1, 5.0, ""),
            p_float("gain", "Gain", 1.0, 0.0, 4.0, ""),
        ],
        "Order: exposure -> white balance -> lift/gamma/gain -> contrast around 18% gray -> hue/saturation. Neutral settings are identity.",
        "FX005",
    );
    let body = r#"
fn rgb2hsv(c_in: vec3<f32>) -> vec3<f32> {
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
fn hsv2rgb(hsv: vec3<f32>) -> vec3<f32> {
  let h = fract(hsv.x) * 6.0;
  let i = floor(h);
  let f = h - i;
  let p = hsv.z * (1.0 - hsv.y);
  let q = hsv.z * (1.0 - hsv.y * f);
  let t = hsv.z * (1.0 - hsv.y * (1.0 - f));
  let m = i32(mod(i, 6.0));
  var c = vec3<f32>(hsv.z, t, p);
  if (m == 1) { c = vec3<f32>(q, hsv.z, p); }
  else if (m == 2) { c = vec3<f32>(p, hsv.z, t); }
  else if (m == 3) { c = vec3<f32>(p, q, hsv.z); }
  else if (m == 4) { c = vec3<f32>(t, p, hsv.z); }
  else if (m == 5) { c = vec3<f32>(hsv.z, p, q); }
  return c;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  var c = unpremul(src);
  c = c * pow(2.0, p_exposure());
  // White balance: warm shifts toward red/yellow, cool toward blue.
  let temp = p_temperature();
  let tintv = p_tint();
  c = c * vec3<f32>(1.0 + temp * 0.25 - tintv * 0.05, 1.0 + temp * 0.05 - tintv * 0.15, 1.0 - temp * 0.25 + tintv * 0.05);
  // lift / gamma / gain
  let lift = p_lift();
  let gamma = max(p_gamma(), 0.01);
  let gain = max(p_gain(), 0.0);
  c = pow(max(c + vec3<f32>(lift), vec3<f32>(0.0)), vec3<f32>(1.0 / gamma)) * gain;
  // contrast around 18% linear gray
  let cont = p_contrast();
  let g = 0.18;
  c = (c - vec3<f32>(g)) * cont + vec3<f32>(g);
  // hue/saturation in HSV
  var hsv = rgb2hsv(max(c, vec3<f32>(0.0)));
  hsv.x = fract(hsv.x + p_hue() / 360.0);
  hsv.y = clamp(hsv.y * p_saturation(), 0.0, 1.0);
  c = hsv2rgb(hsv);
  return premul(max(c, vec3<f32>(0.0)), src.a);
}
"#;
    ("fx_color_adjustment", m, body)
}

pub fn fx006_colored_edges() -> (&'static str, Manifest, &'static str) {
    let m = effect(
        "fx.colored_edges",
        "Colored edges",
        "Print and Pixel",
        "fx_colored_edges",
        QualityClass::M,
        vec![
            p_float("threshold", "Threshold", 0.15, 0.0, 2.0, ""),
            p_float("softness", "Softness", 0.05, 0.0, 1.0, ""),
            p_float("thickness", "Thickness", 1.0, 0.5, 12.0, "output_px"),
            p_color("edgeColor", "Edge color", [0.0, 1.0, 1.0, 1.0]),
            p_float("backgroundMix", "Background mix", 0.0, 0.0, 1.0, ""),
            p_bool("hueByDirection", "Hue by direction", false),
        ],
        "Sobel luminance edges with colored ink. Thicker edges dilate the edge mask.",
        "FX006",
    );
    let body = r#"
fn hsv2rgb2(c: vec3<f32>) -> vec3<f32> {
  let k = fract(c.xxx + vec3<f32>(0.0, 2.0 / 3.0, 1.0 / 3.0));
  let p = abs(k * 6.0 - 3.0);
  return clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)) * (c.y - 1.0) + vec3<f32>(1.0) * 0.0 + clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
}
fn edge_strength(uv: vec2<f32>) -> f32 {
  let e = px_to_uv(max(p_thickness(), 0.5));
  let tl = sample_luma(uv + vec2<f32>(-e.x, -e.y));
  let t0 = sample_luma(uv + vec2<f32>(0.0, -e.y));
  let tr = sample_luma(uv + vec2<f32>(e.x, -e.y));
  let ml = sample_luma(uv + vec2<f32>(-e.x, 0.0));
  let mr = sample_luma(uv + vec2<f32>(e.x, 0.0));
  let bl = sample_luma(uv + vec2<f32>(-e.x, e.y));
  let b0 = sample_luma(uv + vec2<f32>(0.0, e.y));
  let br = sample_luma(uv + vec2<f32>(e.x, e.y));
  let gx = -tl - 2.0 * ml - bl + tr + 2.0 * mr + br;
  let gy = -tl - 2.0 * t0 - tr + bl + 2.0 * b0 + br;
  return sqrt(gx * gx + gy * gy) * 0.25;
}
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let e = px_to_uv(max(p_thickness(), 0.5));
  let s = edge_strength(uv);
  // Dilation of the edge mask at the configured thickness approximates thick
  // lines; sample a few offset directions.
  var best = s;
  let dirs = 6;
  for (var i = 0; i < dirs; i = i + 1) {
    let a = f32(i) / f32(dirs) * 6.2831853;
    best = max(best, edge_strength(uv + vec2<f32>(cos(a), sin(a)) * e * 0.5));
  }
  let soft = max(p_softness(), 0.0001);
  let mask = smoothstep(p_threshold(), p_threshold() + soft, best);
  let col = p_edgeColor().rgb;
  let bg = unpremul(src) * (1.0 - p_backgroundMix());
  let ink = col;
  var out = mix(bg, ink, mask);
  if (p_hueByDirection()) {
    let e2 = px_to_uv(max(p_thickness(), 0.5));
    let dir = vec2<f32>(sample_luma(uv + vec2<f32>(e2.x, 0.0)) - sample_luma(uv - vec2<f32>(e2.x, 0.0)),
                        sample_luma(uv + vec2<f32>(0.0, e2.y)) - sample_luma(uv - vec2<f32>(0.0, e2.y)));
    let h = fract(atan2(dir.y, dir.x) / 6.2831853 + 1.0);
    out = mix(out, hsv2rgb2(vec3<f32>(h, 0.8, 1.0)), mask);
  }
  return premul(out, max(src.a, mask));
}
"#;
    ("fx_colored_edges", m, body)
}
