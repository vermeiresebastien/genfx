// Generated from manifest. Do not edit by hand.
// Shader ABI version 1.

struct NodeParams {
  f: array<vec4<f32>, 2>,
  u: array<vec4<u32>, 1>,
};

fn p_wet() -> f32 { return np.f[0].x; }
fn p_color() -> vec4<f32> { return np.f[1]; }
fn p_transparent() -> bool { return np.u[0].x != 0; }

struct FrameUniform {
  dims: vec4<u32>,
  sampling: vec4<f32>,
  clock: vec4<f32>,
  counters: vec4<u32>,
  music: vec4<f32>,
  controls: vec4<f32>,
};

struct AudioFeatures {
  bands: vec4<f32>,
  extra: vec4<f32>,
  params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: FrameUniform;
@group(0) @binding(1) var<uniform> np: NodeParams;
@group(0) @binding(2) var<uniform> audio: AudioFeatures;



@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var mask_tex: texture_2d<f32>;
@group(1) @binding(2) var src_sampler: sampler;

fn hash21(p: vec2<f32>) -> f32 {
  var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
  p3 += dot(p3, p3.yzx + 33.33);
  return fract((p3.x + p3.y) * p3.z);
}
fn hash22(p: vec2<f32>) -> vec2<f32> {
  var p3 = fract(vec3<f32>(p.xyx) * vec3<f32>(0.1031, 0.1030, 0.0973));
  p3 += dot(p3, p3.yzx + 33.33);
  return fract((p3.xx + p3.yz) * p3.zy);
}
fn hash31(p: vec3<f32>) -> f32 {
  var p3 = fract(p * 0.1031);
  p3 += dot(p3, p3.zyx + 31.32);
  return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  let a = hash21(i);
  let b = hash21(i + vec2<f32>(1.0, 0.0));
  let c = hash21(i + vec2<f32>(0.0, 1.0));
  let d = hash21(i + vec2<f32>(1.0, 1.0));
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn gradient_noise(p: vec2<f32>) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  let ga = hash22(i) * 2.0 - 1.0;
  let gb = hash22(i + vec2<f32>(1.0, 0.0)) * 2.0 - 1.0;
  let gc = hash22(i + vec2<f32>(0.0, 1.0)) * 2.0 - 1.0;
  let gd = hash22(i + vec2<f32>(1.0, 1.0)) * 2.0 - 1.0;
  let va = dot(ga, f);
  let vb = dot(gb, f - vec2<f32>(1.0, 0.0));
  let vc = dot(gc, f - vec2<f32>(0.0, 1.0));
  let vd = dot(gd, f - vec2<f32>(1.0, 1.0));
  return mix(mix(va, vb, u.x), mix(vc, vd, u.x), u.y) * 0.5 + 0.5;
}
fn fbm(p_in: vec2<f32>, octaves: i32, lacunarity: f32, persistence: f32) -> f32 {
  var p = p_in;
  var amp = 0.5;
  var sum = 0.0;
  var norm = 0.0;
  let n = clamp(octaves, 1, 8);
  for (var i = 0; i < n; i = i + 1) {
    sum += amp * value_noise(p);
    norm += amp;
    p = p * lacunarity;
    amp *= persistence;
  }
  if (norm <= 0.0) { return 0.0; }
  return sum / norm;
}
fn turbulence(p_in: vec2<f32>, octaves: i32, lacunarity: f32, persistence: f32) -> f32 {
  var p = p_in;
  var amp = 0.5;
  var sum = 0.0;
  var norm = 0.0;
  let n = clamp(octaves, 1, 8);
  for (var i = 0; i < n; i = i + 1) {
    sum += amp * abs(value_noise(p) * 2.0 - 1.0);
    norm += amp;
    p = p * lacunarity;
    amp *= persistence;
  }
  if (norm <= 0.0) { return 0.0; }
  return sum / norm;
}
fn ridged(p_in: vec2<f32>, octaves: i32, lacunarity: f32, persistence: f32) -> f32 {
  var p = p_in;
  var amp = 0.5;
  var sum = 0.0;
  var norm = 0.0;
  let n = clamp(octaves, 1, 8);
  for (var i = 0; i < n; i = i + 1) {
    sum += amp * (1.0 - abs(value_noise(p) * 2.0 - 1.0));
    norm += amp;
    p = p * lacunarity;
    amp *= persistence;
  }
  if (norm <= 0.0) { return 0.0; }
  return sum / norm;
}
// Looping fBm: uses a 3D lattice cycle so tiling is exact when `period` > 0.
fn periodic_value_noise(p: vec2<f32>, period: vec2<f32>) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = f * f * (3.0 - 2.0 * f);
  let pp = max(period, vec2<f32>(1.0));
  let i0 = fmodv(i, pp);
  let i1 = fmodv(i + vec2<f32>(1.0, 1.0), pp);
  let a = hash21(i0);
  let b = hash21(vec2<f32>(i1.x, i0.y));
  let c = hash21(vec2<f32>(i0.x, i1.y));
  let d = hash21(i1);
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn periodic_fbm(p_in: vec2<f32>, period_in: vec2<f32>, octaves: i32) -> f32 {
  var p = p_in;
  var per = period_in;
  var amp = 0.5;
  var sum = 0.0;
  var norm = 0.0;
  let n = clamp(octaves, 1, 8);
  for (var i = 0; i < n; i = i + 1) {
    sum += amp * periodic_value_noise(p, per);
    norm += amp;
    amp *= 0.5;
    p = p * 2.0;
    per = per * 2.0;
  }
  if (norm <= 0.0) { return 0.0; }
  return sum / norm;
}

// WGSL lacks a math `fmod()` builtin (it has `%`), so provide the float and
// vector helpers our kernels use. `%` in WGSL is the remainder; these give the
// Euclidean modulo expected by the effect math.
fn fmod_f(a: f32, b: f32) -> f32 { return a - b * floor(a / b); }
fn fmod(a: f32, b: f32) -> f32 { return a - b * floor(a / b); }
fn fmodv(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return a - b * floor(a / b); }
fn rot2(a: f32) -> mat2x2<f32> {
  let c = cos(a);
  let s = sin(a);
  return mat2x2<f32>(c, -s, s, c);
}
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
  let lo = c / 12.92;
  let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
  return select(hi, lo, c <= vec3<f32>(0.04045));
}
fn linear_to_srgb(c_in: vec3<f32>) -> vec3<f32> {
  let c = clamp(c_in, vec3<f32>(0.0), vec3<f32>(1.0));
  let lo = c * 12.92;
  let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
  return select(hi, lo, c <= vec3<f32>(0.0031308));
}
// Unpremultiply with epsilon; returns straight RGB.
fn unpremul(c: vec4<f32>) -> vec3<f32> {
  if (c.a <= 0.00001) { return vec3<f32>(0.0); }
  return c.rgb / c.a;
}
fn premul(rgb: vec3<f32>, a: f32) -> vec4<f32> {
  return vec4<f32>(rgb * a, a);
}
fn luminance_from_rgb(c: vec3<f32>) -> f32 {
  return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Folds a processed RGBA (premultiplied) over the original source by wet*mask.
fn final_mix(src: vec4<f32>, processed: vec4<f32>, wet: f32, mask: f32) -> vec4<f32> {
  return mix(src, processed, clamp(wet * mask, 0.0, 1.0));
}
fn blend_normal(src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
  return src + dst * (1.0 - src.a);
}

// Input textures are declared as `src_tex`, `mask_tex`.
fn sample_clamp(uv: vec2<f32>) -> vec4<f32> {
  return textureSampleLevel(src_tex, src_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}
fn sample_mirror(uv: vec2<f32>) -> vec4<f32> {
  return textureSampleLevel(src_tex, src_sampler, mirror_uv(uv), 0.0);
}
fn sample_repeat(uv: vec2<f32>) -> vec4<f32> {
  return textureSampleLevel(src_tex, src_sampler, fract(uv + vec2<f32>(1024.0)), 0.0);
}
fn mirror_uv(uv: vec2<f32>) -> vec2<f32> {
  let i = floor(uv);
  let f = fract(uv);
  let even = fmodv(i, vec2<f32>(2.0)) == vec2<f32>(0.0);
  let m = select(f, vec2<f32>(1.0) - f, even);
  return m;
}
fn luma_lin(c: vec3<f32>) -> f32 {
  return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}
fn sample_luma(uv: vec2<f32>) -> f32 {
  let c = textureSampleLevel(src_tex, src_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
  if (c.a <= 0.0001) { return 0.0; }
  return luma_lin(c.rgb / c.a);
}

fn out_size() -> vec2<f32> {
  return vec2<f32>(f32(frame.dims.x), f32(frame.dims.y));
}
fn inv_size() -> vec2<f32> {
  return frame.sampling.xy;
}
fn time_seconds() -> f32 {
  return frame.clock.x + frame.clock.y;
}
fn frame_index() -> f32 {
  return f32(frame.counters.x);
}
fn node_seed() -> u32 {
  return frame.counters.z;
}
fn seedf() -> f32 {
  return f32(frame.counters.z % 100003u) * 0.0001;
}
fn audio_rms() -> f32 { return audio.extra.x; }
fn audio_onset() -> f32 { return audio.extra.y; }
fn audio_bass() -> f32 { return audio.bands.x; }
fn audio_mid() -> f32 { return audio.bands.y; }
fn audio_treble() -> f32 { return audio.bands.z; }
fn beat_phase() -> f32 { return frame.music.x; }
fn quality_scale() -> f32 { return frame.controls.y; }
// Reference-pixel -> uv size used for spatial parameters. Preview scale is
// compensated so pixel radii match the export appearance.
fn px_to_uv(px: f32) -> vec2<f32> {
  return vec2<f32>(px) * inv_size() * frame.sampling.zw;
}


fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = p_color();
  return premul(c.rgb, c.a);
}


@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
  var pos = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -3.0),
    vec2<f32>(-1.0, 1.0),
    vec2<f32>(3.0, 1.0)
  );
  return vec4<f32>(pos[idx], 0.0, 1.0);
}


@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
  let uv = frag_coord.xy * inv_size();
  let src = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  let m = textureSampleLevel(mask_tex, src_sampler, uv, 0.0).r;
  let processed = process(uv, src);
  return final_mix(src, processed, p_wet(), m);
}

