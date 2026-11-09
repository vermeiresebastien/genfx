//! Vector scene / SVG / Lottie generators (G030-G045) and typography (G070-G079).
//!
//! These render through the vector path (CPU rasterization for SVG/Lottie and
//! the bundled block font for text) and additionally expose mask/geometry
//! outputs. Shader bodies present the rasterized vector layer texture.

use crate::builders::*;
use crate::manifest::*;

macro_rules! vgen {
    ($id:expr, $name:expr, $cat:expr, $shader:expr, $q:expr, $req:expr, $time:expr, $help:expr, $inputs:expr, [$($p:expr),* $(,)?]) => {{
        let mut params = vec![p_float("wet", "Opacity", 1.0, 0.0, 1.0, "")];
        params.extend(vec![$($p),*]);
        let mut m = effect($id, $name, $cat, $shader, $q, vec![], $help, $req);
        m.parameters = params;
        m.inputs = $inputs;
        m.outputs = vec![
            color_output(),
            PortDecl { id: "mask".into(), port_type: PortType::Mask, required: false },
            PortDecl { id: "path".into(), port_type: PortType::PathSet, required: false },
        ];
        m.time_model = $time;
        m.generator_id = $id.to_string();
        m.loop_status = "exact".into();
        ($shader, m, PRESENT_BODY)
    }};
}

const PRESENT_BODY: &str = r#"
// Vector layers are rasterized by the vector path and uploaded as `src_tex`;
// this pass presents the layer with the shared wet/opacity control.
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  let c = textureSampleLevel(src_tex, src_sampler, uv, 0.0);
  return c;
}
"#;

pub fn g030_svg() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.svg", "SVG source", "Vector", "gen_vector", QualityClass::M, "G030", TimeModel::Analytic,
        "Safe SVG import rendered through the vector path with viewBox fit/fill/stretch. Groups, paths, gradients, clip/mask and IDs are preserved; scripts, foreignObject and external references are rejected.",
        vec![],
        [p_enum("fit", "Fit", "contain", &["contain", "cover", "stretch"]), p_float("phase", "Loop phase", 0.0, 0.0, 1.0, "cycles"), p_float("speed", "Speed", 0.0, -4.0, 4.0, "cycles/s")]
    )
}

pub fn g031_path_editor() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.shape", "Shape", "Vector", "gen_vector", QualityClass::M, "G031", TimeModel::Analytic,
        "Editable rectangle, ellipse, polygon, star or Bezier path with fill/stroke/dashes and handles.",
        vec![],
        [
            p_enum("shape", "Shape", "ellipse", &["rect", "ellipse", "polygon", "star", "bezier"]),
            p_float("size", "Size", 0.4, 0.01, 1.0, ""),
            p_float("sides", "Sides / points", 5.0, 3.0, 24.0, ""),
            p_float("innerRadius", "Star inner radius", 0.5, 0.05, 1.0, ""),
            p_color("fill", "Fill", [0.9, 0.3, 0.5, 1.0]),
            p_color("stroke", "Stroke", [1.0, 1.0, 1.0, 1.0]),
            p_float("strokeWidth", "Stroke width", 3.0, 0.0, 32.0, "output_px"),
            p_float("rotation", "Rotation", 0.0, -3.1415926, 3.1415926, "rad"),
        ]
    )
}

pub fn g032_trim_paths() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.trim_paths",
        "Trim paths",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G032",
        TimeModel::Analytic,
        "Draw-on trim start/end/offset with length-correct arc-length sampling.",
        vec![],
        [
            p_float("start", "Start", 0.0, 0.0, 1.0, ""),
            p_float("end", "End", 1.0, 0.0, 1.0, ""),
            p_float("offset", "Offset", 0.0, -1.0, 1.0, ""),
            p_bool("sequential", "Sequential subpaths", true),
        ]
    )
}

pub fn g033_follow_path() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.follow_path",
        "Follow path",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G033",
        TimeModel::Analytic,
        "Moves a shape along a PathSet by arc length with tangent orientation.",
        vec![],
        [
            p_float("position", "Position", 0.0, 0.0, 1.0, ""),
            p_bool("tangent", "Orient to tangent", true),
            p_float("speed", "Speed", 0.0, -4.0, 4.0, "cycles/s")
        ]
    )
}

pub fn g034_path_morph() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.path_morph", "Path morph", "Vector", "gen_vector", QualityClass::M, "G034", TimeModel::Analytic,
        "Morphs compatible-topology paths by resampling contours to common points, preserving winding and holes.",
        vec![],
        [p_float("morph", "Morph", 0.0, 0.0, 1.0, ""), p_float("speed", "Speed", 0.0, -4.0, 4.0, "cycles/s"), p_int("samples", "Samples", 256, 16, 4096)]
    )
}

pub fn g035_shape_morph() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.shape_morph", "Shape morph", "Vector", "gen_vector", QualityClass::M, "G035", TimeModel::Analytic,
        "Topology-aware interpolation between circle/square/star outlines without random point shuffling.",
        vec![],
        [p_enum("from", "From", "circle", &["circle", "square", "star"]), p_enum("to", "To", "star", &["circle", "square", "star"]), p_float("speed", "Speed", 0.2, -4.0, 4.0, "cycles/s")]
    )
}

pub fn g036_repeater() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.repeater",
        "Repeater",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G036",
        TimeModel::Analytic,
        "Line/radial/grid placement of a shape with stagger and per-instance transform/color.",
        vec![],
        [
            p_enum("layout", "Layout", "line", &["line", "radial", "grid"]),
            p_int("count", "Count", 16, 1, 10000),
            p_float("stagger", "Stagger", 0.0, 0.0, 1.0, ""),
            p_float("spacing", "Spacing", 0.08, 0.0, 1.0, "")
        ]
    )
}

pub fn g037_vector_deform() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.vector_deform",
        "Vector deform",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G037",
        TimeModel::Analytic,
        "Sine/noise/bend/twist deformation of sampled outlines.",
        vec![],
        [
            p_enum("kind", "Kind", "sine", &["sine", "noise", "bend", "twist"]),
            p_float("amount", "Amount", 0.1, 0.0, 0.5, ""),
            p_float("speed", "Speed", 0.2, 0.0, 4.0, "")
        ]
    )
}

pub fn g038_stroke_fx() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.stroke_fx",
        "Stroke effects",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G038",
        TimeModel::Analytic,
        "Animated width, dashes and a traveling highlight along the stroke.",
        vec![],
        [
            p_float("width", "Width", 3.0, 0.0, 32.0, "output_px"),
            p_float("dash", "Dash", 0.0, 0.0, 1.0, ""),
            p_float("highlight", "Traveling highlight", 0.5, 0.0, 1.0, "")
        ]
    )
}

pub fn g039_gradient_animator() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.gradient_animator",
        "Gradient animator",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G039",
        TimeModel::Analytic,
        "Animates gradient stops, hue and coordinates while keeping gradients as gradients.",
        vec![],
        [
            p_float("hueShift", "Hue shift", 0.0, -1.0, 1.0, ""),
            p_float("speed", "Speed", 0.1, -4.0, 4.0, "")
        ]
    )
}

pub fn g040_vector_mask() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.vector_mask",
        "Vector mask",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G040",
        TimeModel::Analytic,
        "Animated path exposed as a Mask usable by any image or generator.",
        vec![PortDecl {
            id: "source".into(),
            port_type: PortType::Texture2D,
            required: false
        }],
        [
            p_float("feather", "Feather", 2.0, 0.0, 32.0, "output_px"),
            p_bool("invert", "Invert", false)
        ]
    )
}

pub fn g041_vector_particles() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.vector_particles",
        "Vector to particles",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G041",
        TimeModel::FixedStep,
        "Samples along outline/interior into particle positions/normals/colors.",
        vec![],
        [
            p_int("count", "Count", 5000, 100, 200000),
            p_float("spacing", "Spacing", 0.01, 0.001, 0.1, "")
        ]
    )
}

pub fn g042_vector_mesh() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.vector_mesh", "Vector to mesh", "Vector", "gen_vector", QualityClass::M, "G042", TimeModel::Analytic,
        "Flattens curves (tolerance) and triangulates holes with optional extrusion into a Geometry output.",
        vec![], [p_float("tolerance", "Flatten tolerance", 0.5, 0.05, 4.0, "output_px"), p_float("extrusion", "Extrusion", 0.1, 0.0, 2.0, "m")]
    )
}

pub fn g043_lottie() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.lottie", "Lottie source", "Vector", "gen_vector", QualityClass::M, "G043", TimeModel::Analytic,
        "Lottie JSON/.lottie playback with actual frame seek, loop, speed and markers. Unsupported expressions and effects are reported in the import report.",
        vec![],
        [p_bool("loop", "Loop", true), p_float("speed", "Speed", 1.0, 0.0, 8.0, "x"), p_float("phase", "Phase", 0.0, 0.0, 1.0, "cycles")]
    )
}

pub fn g044_icon_scene() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.icon_scene",
        "Animated logo",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G044",
        TimeModel::Analytic,
        "Grouped vector layers with stagger and elastic/easing intro templates.",
        vec![],
        [
            p_float("stagger", "Stagger", 0.1, 0.0, 1.0, ""),
            p_enum(
                "ease",
                "Easing",
                "easeOut",
                &["linear", "easeIn", "easeOut", "elastic"]
            )
        ]
    )
}

pub fn g045_handdrawn() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.handdrawn",
        "Hand-drawn loop",
        "Vector",
        "gen_vector",
        QualityClass::M,
        "G045",
        TimeModel::CanonicalFrame,
        "Deterministic periodic wobble/jitter held at a chosen fps.",
        vec![],
        [
            p_float("amplitude", "Amplitude", 1.5, 0.0, 8.0, "output_px"),
            p_float("fps", "Hold fps", 12.0, 1.0, 60.0, "")
        ]
    )
}

// ---- Typography (G070-G079) ----

pub fn g070_text_plate() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.text", "Text", "Typography", "gen_text", QualityClass::M, "G070", TimeModel::Analytic,
        "Text plate using the bundled block font with size, alignment, tracking and fill/stroke. Text is usable as image, Mask, PathSet and Geometry.",
        vec![],
        [
            p_float("size", "Size", 0.12, 0.02, 0.6, "composition"),
            p_float("tracking", "Tracking", 0.0, -0.2, 0.5, ""),
            p_float("lineHeight", "Line height", 1.2, 0.8, 3.0, ""),
            p_color("fill", "Fill", [1.0, 1.0, 1.0, 1.0]),
            p_color("stroke", "Stroke", [0.0, 0.0, 0.0, 1.0]),
            p_float("strokeWidth", "Stroke width", 0.0, 0.0, 8.0, "output_px"),
            p_enum("align", "Align", "center", &["left", "center", "right"]),
            p_float("opacity", "Text opacity", 1.0, 0.0, 1.0, ""),
            p_enum("contentKey", "Content", "FLUX", &["FLUX", "Hello", "0123", "ABC"]),
        ]
    )
}

pub fn g071_kinetic() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.kinetic",
        "Kinetic type",
        "Typography",
        "gen_text",
        QualityClass::M,
        "G071",
        TimeModel::Analytic,
        "Per-glyph position/rotation/scale/opacity with wave and stagger intro/hold/outro.",
        vec![],
        [
            p_float("stagger", "Stagger", 0.06, 0.0, 0.5, "s"),
            p_float("waveAmp", "Wave amplitude", 0.1, 0.0, 0.5, ""),
            p_enum(
                "ease",
                "Easing",
                "easeOut",
                &["linear", "easeIn", "easeOut", "elastic"]
            )
        ]
    )
}

pub fn g072_typewriter() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.typewriter",
        "Typewriter",
        "Typography",
        "gen_text",
        QualityClass::M,
        "G072",
        TimeModel::Analytic,
        "Reveals shaped clusters with a cursor and deterministic random substitution.",
        vec![],
        [
            p_float("speed", "Chars per second", 12.0, 1.0, 60.0, ""),
            p_bool("cursor", "Cursor", true),
            p_float("scramble", "Scramble", 0.0, 0.0, 1.0, "")
        ]
    )
}

pub fn g073_text_path() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.text_path",
        "Text on path",
        "Typography",
        "gen_text",
        QualityClass::M,
        "G073",
        TimeModel::Analytic,
        "Arc-length placement of glyphs along a PathSet with tangent rotation and scrolling.",
        vec![],
        [
            p_float("offset", "Path offset", 0.0, 0.0, 1.0, ""),
            p_float("scroll", "Scroll speed", 0.0, -4.0, 4.0, "")
        ]
    )
}

pub fn g074_text_3d() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.text_3d", "3D text", "Typography", "gen_vector", QualityClass::M, "G074", TimeModel::Analytic,
        "Extruded glyph outlines with holes, bevel and scene lighting (rendered by the mesh scene path).",
        vec![], [p_float("depth", "Depth", 0.2, 0.0, 2.0, "m"), p_float("bevel", "Bevel", 0.02, 0.0, 0.1, "m"), p_float("yaw", "Yaw", 0.3, -3.14, 3.14, "rad")]
    )
}

pub fn g075_text_particles() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.text_particles",
        "Text particles",
        "Typography",
        "gen_vector",
        QualityClass::M,
        "G075",
        TimeModel::FixedStep,
        "Outline/interior sampled glyph emission with attract-to-text and release events.",
        vec![],
        [
            p_int("count", "Count", 8000, 100, 200000),
            p_float("blend", "Attract blend", 0.5, 0.0, 1.0, "")
        ]
    )
}

pub fn g076_physics_type() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.physics_type", "Physics type", "Typography", "gen_vector", QualityClass::M, "G076", TimeModel::FixedStep,
        "Glyph/word rigid bodies with gravity, bounce and springs. Layout is remembered and reset is deterministic.",
        vec![], [p_float("gravity", "Gravity", 9.81, 0.0, 30.0, "m/s2"), p_float("bounce", "Bounce", 0.4, 0.0, 1.0, "")]
    )
}

pub fn g077_counters() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.counter",
        "Counter / clock",
        "Typography",
        "gen_text",
        QualityClass::L,
        "G077",
        TimeModel::Analytic,
        "Numeric display with format, deterministic timecode or counter source at a fixed fps.",
        vec![],
        [
            p_enum("kind", "Kind", "timecode", &["timecode", "frame", "number"]),
            p_int("decimals", "Decimals", 2, 0, 6),
            p_float("start", "Start", 0.0, -1e6, 1e6, "")
        ]
    )
}

pub fn g078_charts() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.chart",
        "Chart",
        "Typography",
        "gen_vector",
        QualityClass::M,
        "G078",
        TimeModel::Analytic,
        "Bars, lines and radial charts driven by a data source with bounds and labels.",
        vec![PortDecl {
            id: "data".into(),
            port_type: PortType::VectorField,
            required: false
        }],
        [
            p_enum("kind", "Kind", "bars", &["bars", "line", "radial", "radar"]),
            p_int("bars", "Bars", 12, 2, 64),
            p_color("colorA", "Color A", [0.2, 0.8, 1.0, 1.0]),
            p_color("colorB", "Color B", [1.0, 0.4, 0.2, 1.0])
        ]
    )
}

pub fn g079_overlays() -> (&'static str, Manifest, &'static str) {
    vgen!(
        "gen.overlays",
        "Technical overlay",
        "Typography",
        "gen_vector",
        QualityClass::L,
        "G079",
        TimeModel::Analytic,
        "Grids, rulers, dimensions, labels, scan cursors and callout HUD templates.",
        vec![],
        [
            p_float("grid", "Grid spacing", 0.1, 0.01, 0.5, ""),
            p_float("scan", "Scan cursor", 0.3, 0.0, 1.0, ""),
            p_color("color", "Color", [0.3, 1.0, 0.7, 1.0])
        ]
    )
}
