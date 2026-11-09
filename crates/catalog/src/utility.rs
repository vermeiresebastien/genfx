//! Built-in utility nodes shared by all graphs (source, blend, color, mask,
//! output, feedback) and a registry that merges every catalog entry.

use crate::builders::*;
use crate::manifest::*;

pub fn utility_manifests() -> Vec<(Manifest, &'static str)> {
    let mut v = Vec::new();

    // Source input: presents the graph's external ScopeInput texture.
    {
        let mut m = effect("flux.source", "Source", "Input/Output", "node_source", QualityClass::L,
            vec![], "Graph input: presents the source texture supplied by the clip/track/master scope.", "R007");
        m.inputs = vec![];
        v.push((m, SOURCE_BODY));
    }
    // Mask input
    {
        let mut m = effect("flux.mask_input", "Mask", "Mask", "node_mask", QualityClass::L,
            vec![], "Presents a connected mask (white default).", "R007");
        m.inputs = vec![];
        m.outputs = vec![PortDecl { id: "mask".into(), port_type: PortType::Mask, required: true }];
        v.push((m, MASK_BODY));
    }
    // Blend
    {
        let mut m = effect("flux.blend", "Blend", "Blend", "node_blend", QualityClass::L,
            vec![p_enum("mode", "Blend", "normal", &["normal", "add", "screen", "multiply", "overlay", "softLight", "hardLight", "difference", "exclusion"]),
                 p_float("opacity", "Opacity", 1.0, 0.0, 1.0, "")],
            "Composites two inputs with a defined premultiplied blend formula.", "R007");
        m.inputs = vec![
            PortDecl { id: "a".into(), port_type: PortType::Texture2D, required: true },
            PortDecl { id: "b".into(), port_type: PortType::Texture2D, required: false },
        ];
        m.outputs = vec![color_output()];
        v.push((m, BLEND_BODY));
    }
    // Noise (shared analytic feature node)
    {
        let mut m = effect("flux.noise", "Noise", "Generators", "node_noise", QualityClass::M,
            vec![p_float("scale", "Scale", 4.0, 0.1, 64.0, ""), p_int("octaves", "Octaves", 5, 1, 8),
                 p_float("speed", "Speed", 0.1, -4.0, 4.0, "cycles/s"),
                 p_color("low", "Low", [0.0, 0.0, 0.0, 1.0]), p_color("high", "High", [1.0, 1.0, 1.0, 1.0])],
            "Animated fBm noise field, usable standalone as a displacement field or color source.", "R007");
        m.inputs = vec![];
        v.push((m, NOISE_BODY));
    }
    // Color adjustment (always available)
    {
        let m = effect("flux.color", "Color adjustment", "Color", "node_color", QualityClass::L,
            vec![p_float("exposure", "Exposure", 0.0, -10.0, 10.0, "EV"),
                 p_float("contrast", "Contrast", 1.0, 0.0, 4.0, ""),
                 p_float("saturation", "Saturation", 1.0, 0.0, 3.0, "")],
            "Exposure/contrast/saturation adjustment.", "R007");
        v.push((m, COLOR_BODY));
    }
    // Output sink
    {
        let mut m = effect("flux.output", "Output", "Input/Output", "node_output", QualityClass::L,
            vec![], "Graph output sink.", "R007");
        m.inputs = vec![source_port()];
        m.outputs = vec![];
        v.push((m, OUTPUT_BODY));
    }
    // Feedback pair
    {
        let mut m = effect("flux.feedback_write", "Feedback write", "Temporal", "node_feedback", QualityClass::H,
            vec![], "Stores the input into the canonical-frame history for the matching Feedback read node.", "R007");
        m.inputs = vec![source_port()];
        m.time_model = TimeModel::CanonicalFrame;
        v.push((m, FEEDBACK_WRITE));
    }
    {
        let mut m = effect("flux.feedback_read", "Feedback read", "Temporal", "node_feedback", QualityClass::H,
            vec![], "Reads the previous canonical frame written by a Feedback write node (breaks the temporal cycle).", "R007");
        m.inputs = vec![];
        m.time_model = TimeModel::CanonicalFrame;
        v.push((m, FEEDBACK_READ));
    }
    v
}

const SOURCE_BODY: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> { return src; }
"#;
const MASK_BODY: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> { return src; }
"#;
const BLEND_BODY: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> { return src; }
"#;
const NOISE_BODY: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let n = fbm(uv * p_scale() + vec2<f32>(time_seconds() * p_speed()), p_octaves(), 2.0, 0.5);
  let col = mix(p_low().rgb, p_high().rgb, n);
  return premul(col, 1.0);
}
"#;
const COLOR_BODY: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  var c = unpremul(src);
  c = c * pow(2.0, p_exposure());
  c = (c - vec3<f32>(0.18)) * p_contrast() + vec3<f32>(0.18);
  let l = luminance_from_rgb(c);
  c = mix(vec3<f32>(l), c, p_saturation());
  return premul(max(c, vec3<f32>(0.0)), src.a);
}
"#;
const OUTPUT_BODY: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> { return src; }
"#;
const FEEDBACK_WRITE: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> { return src; }
"#;
const FEEDBACK_READ: &str = r#"
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> { return src; }
"#;

/// Build the full catalog from all source modules.
pub fn build_catalog() -> crate::Catalog {
    let mut entries: Vec<crate::CatalogEntry> = Vec::new();
    let mut push = |_key: &'static str, m: Manifest, body: &'static str| {
        let path = m.implementation.path.clone();
        entries.push(crate::CatalogEntry {
            manifest: m,
            source: body.to_string(),
            source_path: path,
        });
    };

    // 28 requested effects/fills
    for (s, m, b) in [
        crate::effects_a::fx001_bloom(),
        crate::effects_a::fx002_bokeh(),
        crate::effects_a::fx003_channel_mixer(),
        crate::effects_a::fx004_chromatic_metal(),
        crate::effects_a::fx005_color_adjustment(),
        crate::effects_a::fx006_colored_edges(),
        crate::effects_b::fx007_dither(),
        crate::effects_b::fx008_filter_preset(),
        crate::effects_b::fx009_gooey(),
        crate::effects_b::fx010_gradient_map(),
        crate::effects_b::fx011_halftone(),
        crate::effects_b::fx012_hatching(),
        crate::effects_b::fx013_lens_distortion(),
        crate::effects_b::fx014_outlines(),
        crate::effects_b::fx015_pattern_refraction(),
        crate::effects_b::fx016_pixel_stretch(),
        crate::effects_b::fx017_pixelate(),
        crate::effects_b::fx018_slice_shift(),
        crate::effects_b::fx019_warp(),
        crate::effects_c::fx020_clouds(),
        crate::effects_c::fx021_concentric(),
        crate::effects_c::fx022_fractal_noise(),
        crate::effects_c::fx023_glowing_wave(),
        crate::effects_c::fx024_mesh_gradient(),
        crate::effects_c::fx025_moire(),
        crate::effects_c::fx026_nebula(),
        crate::effects_c::fx027_pattern_grid(),
        crate::effects_c::fx028_water_caustic(),
    ] {
        push(s, m, b);
    }

    // Additional effect families
    for (s, m, b) in [
        crate::families::fx100_tone(),
        crate::families::fx101_color(),
        crate::families::fx102_noise(),
        crate::families::fx103_blur(),
        crate::families::fx104_glow(),
        crate::families::fx105_chromatic(),
        crate::families::fx106_vhs(),
        crate::families::fx107_crt(),
        crate::families::fx108_glitch(),
        crate::families::fx109_pixel(),
        crate::families::fx110_smear(),
        crate::families::fx111_geometry(),
        crate::families::fx112_glass(),
        crate::families::fx113_materials(),
        crate::families::fx114_texture(),
        crate::families::fx115_print(),
        crate::families::fx116_technical(),
        crate::families::fx117_text_graphic(),
        crate::families::fx118_temporal(),
        crate::families::fx119_feedback(),
        crate::families::fx120_keys(),
        crate::families::fx121_masks(),
        crate::families::fx122_composite(),
        crate::families::fx123_transforms(),
    ] {
        push(s, m, b);
    }

    // 2D / math generators
    for (s, m, b) in [
        crate::generators_a::g001_solid(),
        crate::generators_a::g002_gradient(),
        crate::generators_a::g003_mesh_gradient(),
        crate::generators_a::g004_noise(),
        crate::generators_a::g005_voronoi(),
        crate::generators_a::g006_plasma(),
        crate::generators_a::g007_grid(),
        crate::generators_a::g008_concentric(),
        crate::generators_a::g009_moire(),
        crate::generators_a::g010_pattern_fill(),
        crate::generators_a::g011_topographic(),
        crate::generators_a::g012_clouds_nebula(),
        crate::generators_a::g013_oscilloscope(),
        crate::generators_a::g014_caustics(),
        crate::generators_a::g015_fractals(),
        crate::generators_a::g016_lissajous(),
        crate::generators_a::g017_superformula(),
        crate::generators_a::g018_truchet(),
        crate::generators_a::g019_texture_fields(),
        crate::generators_a::g020_starfield(),
        crate::generators_a::g021_test_signal(),
        crate::generators_a::g022_polar_tunnel(),
        crate::generators_a::g023_lsystem(),
        crate::generators_a::g024_attractor(),
        crate::generators_a::g025_kaleidoscope(),
        crate::generators_a::g026_recursive_tiling(),
        crate::generators_a::g027_automata(),
        crate::generators_a::g028_reaction_diffusion(),
    ] {
        push(s, m, b);
    }

    // Vector + typography
    for (s, m, b) in [
        crate::generators_b::g030_svg(),
        crate::generators_b::g031_path_editor(),
        crate::generators_b::g032_trim_paths(),
        crate::generators_b::g033_follow_path(),
        crate::generators_b::g034_path_morph(),
        crate::generators_b::g035_shape_morph(),
        crate::generators_b::g036_repeater(),
        crate::generators_b::g037_vector_deform(),
        crate::generators_b::g038_stroke_fx(),
        crate::generators_b::g039_gradient_animator(),
        crate::generators_b::g040_vector_mask(),
        crate::generators_b::g041_vector_particles(),
        crate::generators_b::g042_vector_mesh(),
        crate::generators_b::g043_lottie(),
        crate::generators_b::g044_icon_scene(),
        crate::generators_b::g045_handdrawn(),
        crate::generators_b::g070_text_plate(),
        crate::generators_b::g071_kinetic(),
        crate::generators_b::g072_typewriter(),
        crate::generators_b::g073_text_path(),
        crate::generators_b::g074_text_3d(),
        crate::generators_b::g075_text_particles(),
        crate::generators_b::g076_physics_type(),
        crate::generators_b::g077_counters(),
        crate::generators_b::g078_charts(),
        crate::generators_b::g079_overlays(),
    ] {
        push(s, m, b);
    }

    // Image animators
    for (s, m, b) in [
        crate::generators_c::g050_ken_burns(),
        crate::generators_c::g051_corner_pin(),
        crate::generators_c::g052_mesh_warp(),
        crate::generators_c::g053_liquify(),
        crate::generators_c::g054_parallax(),
        crate::generators_c::g055_heightfield(),
        crate::generators_c::g056_image_particles(),
        crate::generators_c::g057_fragmentation(),
        crate::generators_c::g058_slices(),
        crate::generators_c::g059_slit_scan(),
        crate::generators_c::g060_polar_projection(),
        crate::generators_c::g061_collage(),
        crate::generators_c::g062_sprite(),
        crate::generators_c::g063_motion_brush(),
        crate::generators_c::g064_optical_flow(),
        crate::generators_c::g065_image_morph(),
        crate::generators_c::g066_field_extraction(),
        crate::generators_c::g067_droste(),
        crate::generators_c::g068_stop_motion(),
    ] {
        push(s, m, b);
    }

    // Scene / particle / physics / simulation / control / conditional
    for (s, m, b) in crate::generators_d::sdf_rows()
        .into_iter()
        .chain(crate::generators_d::mesh_rows())
        .chain(crate::generators_d::particle_rows())
        .chain(crate::generators_d::physics_rows())
        .chain(crate::generators_d::simulation_rows())
        .chain(crate::generators_d::control_rows())
        .chain(crate::generators_d::conditional_rows())
    {
        push(s, m, b);
    }

    // Utility nodes
    for (m, b) in utility_manifests() {
        let path = m.implementation.path.clone();
        entries.push(crate::CatalogEntry { manifest: m, source: b.to_string(), source_path: path });
    }

    crate::Catalog { entries }
}
