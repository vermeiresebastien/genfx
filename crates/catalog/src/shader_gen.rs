//! WGSL shader source generation from manifests.
//!
//! Every fragment effect shares one entry-point template with:
//! - group 0 bindings 0/1/2: frame uniform, node params, audio features
//! - group 1 bindings: `src_tex`, `mask_tex`, `src_sampler` (+ extra aux)
//!
//! The shader body is a manifest-selected function `process(uv, src)` that
//! returns a premultiplied RGBA processed result; the template handles the
//! final wet*mask mix so every effect gets transparency and wet/dry for free.

use crate::manifest::*;
use crate::pack::params_wgsl;
use crate::wgsl_common;

/// Wrapper that defines inputs, uniform bindings and the fragment entry.
pub struct ShaderSpec {
    pub has_mask: bool,
    pub extra_textures: Vec<String>,
    /// Whether the effect is a generator with no required source.
    pub generator: bool,
}

impl ShaderSpec {
    pub fn effect() -> Self {
        Self {
            has_mask: true,
            extra_textures: vec![],
            generator: false,
        }
    }
    pub fn generator() -> Self {
        Self {
            has_mask: true,
            extra_textures: vec![],
            generator: true,
        }
    }
    pub fn with_extra(mut self, names: &[&str]) -> Self {
        self.extra_textures = names.iter().map(|s| s.to_string()).collect();
        self
    }
}

pub fn generate_shader(entry: &crate::CatalogEntry) -> String {
    let m = &entry.manifest;
    let spec = if m.inputs.iter().any(|p| p.id == "source" && p.required) {
        ShaderSpec::effect()
    } else {
        ShaderSpec::generator()
    };
    generate_with(entry, spec)
}

pub fn generate_with(entry: &crate::CatalogEntry, spec: ShaderSpec) -> String {
    let m = &entry.manifest;
    let (params_src, _layout) = params_wgsl(&m.parameters);
    let body = &entry.source;

    let mut s = String::new();
    s.push_str("// Generated from manifest. Do not edit by hand.\n// Shader ABI version 1.\n\n");
    s.push_str(&params_src);
    s.push_str(
        r#"
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

"#,
    );
    s.push_str("\n");

    if spec.generator {
        s.push_str(
            r#"
@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var mask_tex: texture_2d<f32>;
@group(1) @binding(2) var src_sampler: sampler;
@group(1) @binding(3) var aux_tex: texture_2d<f32>;
"#,
        );
    } else {
        s.push_str(
            r#"
@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var mask_tex: texture_2d<f32>;
@group(1) @binding(2) var src_sampler: sampler;
@group(1) @binding(3) var aux_tex: texture_2d<f32>;
"#,
        );
    }
    for (i, name) in spec.extra_textures.iter().enumerate() {
        s.push_str(&format!(
            "@group(1) @binding({}) var {name}: texture_2d<f32>;\n",
            4 + i
        ));
    }

    s.push_str(wgsl_common::HASH);
    s.push_str(wgsl_common::NOISE);
    s.push_str(wgsl_common::COMMON);
    s.push_str(wgsl_common::BLEND);
    s.push_str(wgsl_common::SAMPLE);

    // Frame-context helpers shared by all effects.
    s.push_str(
        r#"
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

"#,
    );

    s.push_str(body);
    s.push_str("\n");

    // Fullscreen triangle vertex stage. No vertex buffers: 3 vertices emitted
    // from the vertex index, covering the whole clip space.
    s.push_str(
        r#"
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
  var pos = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -3.0),
    vec2<f32>(-1.0, 1.0),
    vec2<f32>(3.0, 1.0)
  );
  return vec4<f32>(pos[idx], 0.0, 1.0);
}

"#,
    );

    // Entry point. `process` receives uv and source RGBA and returns premultiplied RGBA.
    s.push_str(
        r#"
@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
  let uv = frag_coord.xy * inv_size();
  let src = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  let m = textureSampleLevel(mask_tex, src_sampler, uv, 0.0).r;
  let processed = process(uv, src);
  return final_mix(src, processed, p_wet(), m);
}
"#,
    );
    if !m.outputs.iter().any(|o| o.id == "color") {
        // generators output color as well
    }
    s
}

/// A minimal WGSL validity heuristic used by the catalog linter. Real
/// validation is done by Naga (`wgpu::naga`) in `tests` and by the GPU device.
pub fn quick_wgsl_check(src: &str) -> Result<(), String> {
    let open = src.matches('{').count();
    let close = src.matches('}').count();
    if open != close {
        return Err(format!("unbalanced braces: {open} open, {close} close"));
    }
    if !src.contains("@fragment") {
        return Err("missing @fragment entry".into());
    }
    if !src.contains("fn fs_main") {
        return Err("missing fs_main".into());
    }
    if src.contains("p_wet(") && !src.contains("fn p_wet()") {
        return Err("shader uses wet accessor but manifest has no wet parameter".into());
    }
    Ok(())
}

// Keep `Manifest` referenced from this module for rustdoc cohesion.
#[allow(dead_code)]
fn _uses(_m: &Manifest) {}
