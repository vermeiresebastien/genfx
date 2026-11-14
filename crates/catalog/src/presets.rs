//! Preset recipes (EFFECTS 4) expressed as editable graph templates.
//! A preset is a node list plus edges; inserting it into a project creates real,
//! editable nodes with new IDs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetNode {
    pub type_id: String,
    /// Index into the graph's node list; edges reference these indices.
    pub index: usize,
    pub params: serde_json::Map<String, serde_json::Value>,
    pub position: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetEdge {
    pub from: usize,
    pub from_port: String,
    pub to: usize,
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub requirement_refs: Vec<String>,
    pub nodes: Vec<PresetNode>,
    pub edges: Vec<PresetEdge>,
    pub output: usize,
    pub macros: Vec<String>,
}

fn n(index: usize, type_id: &str, params: &[(&str, serde_json::Value)]) -> PresetNode {
    let mut map = serde_json::Map::new();
    for (k, v) in params {
        map.insert(k.to_string(), v.clone());
    }
    PresetNode {
        type_id: type_id.to_string(),
        index,
        params: map,
        position: [(index as f32 % 4.0) * 220.0, (index as f32 / 4.0) * 140.0],
    }
}

fn chain(types: &[&str], ids: &str) -> Preset {
    let catalog = shared_catalog();
    let manifest = |t: &str| catalog.get(t).map(|e| &e.manifest);

    // Recipes were authored as "effect on footage". In the node editor there is
    // no footage, so a `flux.source` root is a passthrough of transparent black
    // and the whole chain renders black. Substitute a generator that gives the
    // recipe something real to work on, chosen from its id.
    let substitute: &str = if ids.contains("glass")
        || ids.contains("chrome")
        || ids.contains("thermal")
        || ids.contains("metal")
        || ids.contains("bloom")
    {
        "gen.mesh_gradient_fill"
    } else if ids.contains("technical")
        || ids.contains("wireframe")
        || ids.contains("blueprint")
        || ids.contains("industrial")
        || ids.contains("ascii")
        || ids.contains("code_rain")
        || ids.contains("mosaic")
    {
        // These modes derive structure from luminance, so they need a bright,
        // high-contrast field (a dark grid renders as black).
        "gen.texture_field"
    } else {
        "gen.plasma"
    };

    let types: Vec<&str> = types
        .iter()
        .map(|t| if *t == "flux.source" { substitute } else { *t })
        .collect();
    let types = types.as_slice();

    // If a recipe is a bare effect with nothing feeding it, prepend the seed
    // generator (handled by the auto-prepend below).

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut next = 0usize;

    // If the first node is an effect that needs an image input, prepend the seed
    // generator so the preset starts from something visible.
    let first = types.first().copied().unwrap_or("gen.solid");
    let first_needs_source = manifest(first)
        .map(|m| m.inputs.iter().any(|p| p.id == "source"))
        .unwrap_or(false);
    if first != "flux.source" && first_needs_source {
        nodes.push(n(next, substitute, &[]));
        next += 1;
    }

    let mut prev: Option<(usize, &str)> = if next == 1 {
        Some((0, substitute))
    } else {
        None
    };
    for t in types {
        let idx = next;
        next += 1;
        nodes.push(n(idx, t, &[]));

        if let Some((pidx, ptype)) = prev {
            // Only connect when the target has an input and the types match the
            // engine's strict port-type check. Generators have no `source` input,
            // so they start their own branch; a required input left unconnected is
            // caught by validation rather than guessed at here.
            if let (Some((out_id, out_ty)), Some((in_id, in_ty))) =
                (pick_output(manifest(ptype)), pick_input(manifest(t)))
            {
                if out_ty == in_ty {
                    edges.push(PresetEdge {
                        from: pidx,
                        from_port: out_id,
                        to: idx,
                        to_port: in_id,
                    });
                }
            }
        }
        prev = Some((idx, t));
    }

    let sink = next;
    nodes.push(n(sink, "flux.output", &[]));
    if let Some((pidx, ptype)) = prev {
        if let (Some((out_id, out_ty)), Some((in_id, in_ty))) = (
            pick_output(manifest(ptype)),
            pick_input(manifest("flux.output")),
        ) {
            if out_ty == in_ty {
                edges.push(PresetEdge {
                    from: pidx,
                    from_port: out_id,
                    to: sink,
                    to_port: in_id,
                });
            }
        }
    }

    Preset {
        id: ids.into(),
        name: ids.rsplit('.').next().unwrap_or(ids).replace('_', " "),
        description: format!("Graph chain: {}", types.join(" -> ")),
        requirement_refs: vec!["FX001".into()],
        nodes,
        edges,
        output: sink,
        macros: vec![],
    }
}

fn brik_recipe(id: &str, name: &str, description: &str, types: &[&str]) -> Preset {
    let mut preset = chain(types, id);
    preset.name = name.into();
    preset.description = description.into();
    preset.requirement_refs = vec!["BRIK01".into(), "FX001".into()];
    preset.macros = vec![
        "Randomize generator parameters".into(),
        "Blend with the active graph".into(),
    ];
    preset
}

fn brik_library_recipe(index: usize, name: &str) -> Preset {
    let lower = name.to_ascii_lowercase();
    let types: &[&str] = if lower.contains("text")
        || lower.contains("type")
        || lower.contains("glyph")
        || lower.contains("letter")
        || lower.contains("word")
        || lower.contains("terminal")
        || lower.contains("kinetic")
    {
        &["gen.kinetic", "fx.glow", "fx.chromatic"]
    } else if lower.contains("glass")
        || lower.contains("liquid")
        || lower.contains("iridescent")
        || lower.contains("prism")
        || lower.contains("crystal")
        || lower.contains("metal")
    {
        &["gen.mesh_gradient_fill", "fx.glass", "fx.chromatic"]
    } else if lower.contains("grid")
        || lower.contains("matrix")
        || lower.contains("chart")
        || lower.contains("data")
        || lower.contains("scatter")
        || lower.contains("pixel")
    {
        &["gen.grid", "fx.color_adjustment", "fx.dither"]
    } else if lower.contains("fluid")
        || lower.contains("ripple")
        || lower.contains("wave")
        || lower.contains("swirl")
        || lower.contains("ambient")
        || lower.contains("motion")
    {
        &["gen.clouds", "fx.warp", "fx.bloom"]
    } else if lower.contains("3d")
        || lower.contains("shape")
        || lower.contains("geometry")
        || lower.contains("logo")
        || lower.contains("poster")
    {
        &["gen.sdf_blob", "fx.materials", "fx.bloom"]
    } else if lower.contains("retro")
        || lower.contains("thermal")
        || lower.contains("blueprint")
        || lower.contains("ascii")
        || lower.contains("printer")
        || lower.contains("halftone")
    {
        &["gen.texture_field", "fx.crt", "fx.print"]
    } else if lower.contains("particle") {
        &["gen.text_particles", "fx.glow", "fx.bloom"]
    } else {
        &["gen.mesh_gradient_fill", "fx.chromatic", "fx.glow"]
    };
    let id = format!("preset.brik.gallery_{:04}", index + 1);
    let description = format!(
        "Brik gallery tool concept: {name}. Native Flux graph with editable generator and effects."
    );
    brik_recipe(&id, name, &description, types)
}

const BRIK_GALLERY_NAMES: &[&str] = &[
    "Anarchy Kinetic Type",
    "Wall of Eyes",
    "text Kaleidoscope",
    "Text Swarm",
    "Liquid Swirl Background",
    "Orbital Ribbon Flow",
    "Iridescent Motion Veil",
    "Fluid AT",
    "Stop Motion Lines",
    "Terminal",
    "Timeline Gallery",
    "Liquid Ripple Vision",
    "Roaps",
    "Prismatic Pixel Press",
    "FoldOut Gallery",
    "Draw with stickers",
    "Checker Grid",
    "PN PLTR",
    "Risograph Overprint Poster",
    "Throwing Words",
    "Thermal Reactive-Camera",
    "Kinetic Pulse Postcard",
    "Minesweeper",
    "Kinetic Editorial Type",
    "echo",
    "Percentage data",
    "Chroma Ink Reveal",
    "TH_FR - DITTERTOOL TEST",
    "Radial chart",
    "Unified Wave Scroll",
    "彩色流动背景版",
    "Iridescent Glass Topology",
    "Vector Poster",
    "linetool",
    "3D Poster",
    "Coordinate Scatter 3D",
    "Hole-Punch Masking",
    "LNS",
    "Genesis Generator",
    "Cascade Type",
    "Glass Tool",
    "Dense Shape Formations",
    "Optical Geometry Studio",
    "Glitch Feedback Type",
    "Kinetic Field",
    "Glyph Matrix",
    "Shani's kaleidoscope",
    "Type for fun",
    "Dynamic Stripes",
    "Interactive Photo Cube",
    "Hamsa La Vista",
    "Letter Flip Composition",
    "Shift Grid",
    "404 Ripple",
    "growth",
    "Gallery Ad in Motion",
    "GOOEY TEXT",
    "Kinetic Pulse Motion",
    "Sketchbook 002",
    "png to 3d logo",
    "Interactive Shader Backdrop",
    "Living Glyph Automata",
    "Glass Spheres",
    "Running Skull",
    "Kinetic Hebrew",
    "dagim",
    "Liquid Metal Brush",
    "Symmetric Ink Poster",
    "Terminal text",
    "GRID MORPHING WORD",
    "Ambient Video Canvas",
    "30k — Flip Mosaic",
    "TikTok Character Reveal",
    "Floating Gravity Gallery",
    "flow gallery",
    "PSYCADELLIC",
    "Cybersigil Topo Filter",
    "3Dflag",
    "Klein Letter Particles",
    "Soft Focus Backdrop",
    "Fluid in Motion",
    "Text Layer Type",
    "playgroundwebsite",
    "Monster Alphabet",
    "Interactive Halftone Grid",
    "404 Grid",
    "wave project gallery",
    "Elastic Grid Typography",
    "Face Rig Collage",
    "60s Trash Palace",
    "Pixelated Motion",
    "CRCL",
    "Crystalline Morph",
    "Chromatic Stellar Ripples",
    "Sale Stickers",
    "Font Asset Resolver",
    "Typography DVD",
    "Dissolve",
    "Format & Canvas Aspect Ratio",
    "Master Linear Gallery",
    "Square Typography Canvas",
    "Hover Crate Gallery",
    "ASCII",
    "Tactile Press Type",
    "Stretch Type Stage",
    "Blueprint video scanner",
    "Dotted Pumpkin",
    "Thermal Camera",
    "Bead Net",
    "Chromatic Halftone Video",
    "Sinusoidal Map Viz",
    "color grid video",
    "Sequin Strands",
    "Spiral 3D Ticker",
    "type for fun 03",
    "Particle Gallery",
    "Flags",
    "Envelope open",
    "Terminal rain / NEO",
    "Kinetic Matrix Poster",
    "Printer",
    "Bicycle Tova",
    "Gravity Drop Type",
    "Retro Signal Room",
];

const BRIK_COMMUNITY_NAMES: &[&str] = &[
    "Remix of Audio Visualizer",
    "Spiral 3D Ticker",
    "Mechanical Keystroke UI",
    "Flow Stack Gallery",
    "Smiling Sticker",
    "y2k digicam",
    "Magnetic Confetti Swarm",
    "Remix of Rack Tom 3D",
    "Extruded Word",
    "Morphogenesis Generative",
    "Threshold Shift - Image Transformation",
    "Liquid Glass Cursor",
    "Morphing Galleries",
    "Vertical Card Scroll",
    "Dynamic Flow Gallery",
    "Remix of Logo anatomy breakdown tool",
    "Accordion Grid Gallery",
    "Pixel Mosaic Effect",
    "Chrome Bearbrick 3D",
    "Inline Ripple Type",
    "Stacked Letters",
    "Stipple Extrude Brush",
    "Products Gallery",
    "Kinetic Varsity Brand",
    "Neural Net Text Scanner Effect",
    "Recursive Generator",
    "Depth Window",
    "Emoji Swarm Camera",
    "Stop-Motion Highlighter",
    "Letter Pool",
    "Audioreactive blobs",
    "Flowing Text Trail",
    "Sticky Gummy Type",
    "Liquid Swirl Background",
    "Stretch Gallery",
    "Cut-Out Letters",
    "Sale Stickers",
    "Terminal Script UI - Background Anomation",
    "彩色流动背景版",
    "Letter Cube",
    "Rubik’s Cube Portraits",
    "Funky Text",
    "Gravity Drop Layout",
    "Remix of Fisheye Lens Camera",
    "Elastic Grid Typography",
    "Letter Flip Composition",
    "Woodland Disco Motion",
    "Simulated Interdimensional Travel",
    "Remix of LOADER",
    "blob poster",
    "Print Stack",
    "Morphogenesis Generative Multiverse",
    "Digital Receipt",
    "Kinetic Editorial Type",
    "Swatch Letters",
    "Untitled Tool",
    "Metaball Type",
    "Xerox Type",
    "Audio Visualizer",
    "3D Orbit Product Showcase",
    "Circular Mask Filter",
    "Proximity connecting grid",
    "Dotted Letters",
    "Metallic Reveal Cursor",
    "Matter Bloom",
    "Modular Glyph Ascii",
    "Stamp Grid",
    "Bento Drift",
    "Aperture Dive Gallery",
    "Dissolve Type Echo",
    "Particle Typography",
    "Expanding Timeline Gallery",
    "Seismic Spectrograph Viz",
    "Floating Cutout Overlay",
    "Kaleidoscope Warp Filter",
    "Missing Piece",
    "Urban CRT Gallery",
    "Sale Patch Grid",
    "CHROMATIC SIGNAL FIELD",
    "Sticky Cuts",
    "Starlight Tracker",
    "Sticker Storm",
    "Fold Out Gallery V2",
    "Kinetic Typography Cube",
];

fn shared_catalog() -> &'static crate::Catalog {
    static C: std::sync::OnceLock<crate::Catalog> = std::sync::OnceLock::new();
    C.get_or_init(crate::build_catalog)
}

/// Preferred image output: `color`, else the first output.
fn pick_output(m: Option<&crate::Manifest>) -> Option<(String, crate::manifest::PortType)> {
    let m = m?;
    m.outputs
        .iter()
        .find(|p| p.id == "color")
        .or_else(|| m.outputs.first())
        .map(|p| (p.id.clone(), p.port_type))
}

/// Preferred image input: required `source`, then optional `source`, then `a`,
/// then the first input (so effects with a differently named image port work).
fn pick_input(m: Option<&crate::Manifest>) -> Option<(String, crate::manifest::PortType)> {
    let m = m?;
    m.inputs
        .iter()
        .find(|p| p.id == "source" && p.required)
        .or_else(|| m.inputs.iter().find(|p| p.id == "source"))
        .or_else(|| m.inputs.iter().find(|p| p.id == "a"))
        .or_else(|| m.inputs.first())
        .map(|p| (p.id.clone(), p.port_type))
}

pub fn all_presets() -> Vec<Preset> {
    let mut v = Vec::new();

    // Bass Impact: blob -> chromatic metal -> bloom (bass->deformation via mapping)
    {
        let mut p = chain(
            &["gen.sdf_blob", "fx.chromatic_metal", "fx.bloom"],
            "preset.bass_impact",
        );
        p.name = "Bass Impact".into();
        p.description =
            "Blob -> Chromatic metal -> Bloom. Bass drives deformation, onset triggers a pulse."
                .into();
        p.requirement_refs = vec!["FX001".into(), "G081".into(), "R014".into()];
        p.macros = vec![
            "bass->blob.radius".into(),
            "onset->bloom.intensity".into(),
            "treble->grain".into(),
        ];
        if let Some(node) = p.nodes.iter_mut().find(|n| n.type_id == "gen.sdf_blob") {
            node.params
                .insert("bassAmount".into(), serde_json::json!(1.2));
        }
        v.push(p);
    }
    v.push(chain(
        &["flux.source", "fx.chromatic", "fx.vhs", "fx.crt"],
        "preset.vhs_tracking",
    ));
    v.push(chain(
        &["flux.source", "fx.crt", "fx.noise"],
        "preset.crt_phosphor",
    ));
    v.push(chain(
        &[
            "flux.source",
            "fx.color_adjustment",
            "fx.halftone",
            "fx.noise",
        ],
        "preset.editorial_halftone",
    ));
    v.push(chain(&["flux.source", "fx.print"], "preset.cmyk_print"));
    v.push(chain(
        &["flux.source", "fx.tone", "fx.print", "fx.noise"],
        "preset.risograph",
    ));
    v.push(chain(&["flux.source", "fx.dither"], "preset.sierra_dither"));
    v.push(chain(
        &["flux.source", "fx.pixel_ops"],
        "preset.modular_pixel_dissolve",
    ));
    v.push(chain(
        &["flux.source", "fx.technical"],
        "preset.industrial_ct",
    ));
    v.push(chain(
        &["flux.source", "fx.gradient_map", "fx.bloom"],
        "preset.thermal_vision",
    ));
    v.push(chain(
        &["flux.source", "fx.technical"],
        "preset.cad_wireframe",
    ));
    v.push(chain(
        &["flux.source", "fx.technical"],
        "preset.technical_cutaway",
    ));
    v.push(chain(
        &["flux.source", "fx.pattern_refraction"],
        "preset.reeded_glass",
    ));
    v.push(chain(
        &["flux.source", "fx.pattern_refraction"],
        "preset.fluted_glass",
    ));
    v.push(chain(
        &["flux.source", "fx.pattern_refraction"],
        "preset.microprism_glass",
    ));
    v.push(chain(
        &["flux.source", "fx.chromatic_metal", "fx.materials"],
        "preset.liquid_chrome_metal",
    ));
    v.push(chain(
        &["gen.water_caustic", "flux.color"],
        "preset.water_caustics",
    ));
    v.push(chain(&["gen.fractal_noise"], "preset.fractal_noise"));
    v.push(chain(&["gen.clouds"], "preset.clouds"));
    v.push(chain(&["gen.nebula"], "preset.nebula"));
    v.push(chain(&["gen.mesh_gradient_fill"], "preset.mesh_gradient"));
    v.push(chain(&["gen.moire"], "preset.moire"));
    v.push(chain(&["gen.glowing_wave"], "preset.glowing_wave"));
    v.push(chain(&["gen.pattern_grid"], "preset.pattern_grid"));
    v.push(chain(&["gen.concentric"], "preset.concentric_patterns"));
    v.push(chain(&["gen.texture_field"], "preset.paper_texture"));
    v.push(chain(&["fx.technical"], "preset.blueprint"));
    v.push(chain(&["fx.text_graphic"], "preset.ascii_art"));
    v.push(chain(&["gen.topographic"], "preset.topographic_contours"));
    v.push(chain(&["fx.text_graphic"], "preset.shapes_mosaic"));
    v.push(chain(&["fx.text_graphic"], "preset.code_rain"));
    v.push(chain(
        &["flux.source", "fx.slice_shift", "fx.chromatic", "fx.glitch"],
        "preset.cyberpunk_glitch",
    ));
    v.push(chain(
        &["flux.source", "fx.glow", "fx.bloom"],
        "preset.bloom_and_halo",
    ));
    v.push(chain(
        &["gen.fractal_noise"],
        "preset.fractal_noise_template",
    ));
    v.push(chain(&["gen.nebula"], "preset.nebula_template"));
    v.push(chain(&["gen.aurora"], "preset.aurora"));
    {
        let mut p = chain(
            &["gen.particle_emitter", "gen.particle_render", "fx.bloom"],
            "preset.flow_field_particles",
        );
        p.description = "Curl field -> particles -> trails -> bloom.".into();
        p.requirement_refs = vec!["G110".into(), "G112".into(), "FX001".into()];
        v.push(p);
    }
    v.push(chain(
        &["gen.sdf_blob", "fx.chromatic_metal", "fx.bloom"],
        "preset.liquid_blob",
    ));
    v.push(chain(
        &["gen.sdf_blob", "gen.sdf_material", "fx.bloom"],
        "preset.metaball_glow",
    ));
    v.push(chain(
        &["flux.source", "fx.crt", "fx.vhs"],
        "preset.crt_vhs_mix",
    ));
    v.push(chain(&["gen.reaction_diffusion"], "preset.reaction_skin"));

    // Brik workspace recipes. These are intentionally native Flux graphs: every
    // recipe remains editable, can be merged into the active graph, and uses
    // only catalog nodes that have a renderer-backed implementation.
    let brik = [
        (
            "preset.brik_01_pn_pltr",
            "PN PLTR",
            "Poster-like color field with chromatic separation.",
            &["gen.mesh_gradient_fill", "fx.chromatic"][..],
        ),
        (
            "preset.brik_02_glass_spheres",
            "Glass Spheres",
            "Spherical glass treatment with highlights and bloom.",
            &["gen.sdf_blob", "fx.glass", "fx.bloom"][..],
        ),
        (
            "preset.brik_03_velvet_drape_3d",
            "Velvet Drape 3D",
            "Soft dimensional folds built from a shaded field and warp.",
            &["gen.mesh_gradient_fill", "fx.warp", "fx.bloom"][..],
        ),
        (
            "preset.brik_04_orbital_ribbon_flow",
            "Orbital Ribbon Flow",
            "Animated ribbon-like waves with glow and bloom.",
            &["gen.glowing_wave", "fx.glow", "fx.bloom"][..],
        ),
        (
            "preset.brik_05_son_france",
            "Son France",
            "Arthouse color treatment for a cinematic visual surface.",
            &["gen.mesh_gradient_fill", "fx.color_adjustment", "fx.glow"][..],
        ),
        (
            "preset.brik_06_calligram_type",
            "Calligram Type",
            "Typography-led composition with a print texture finish.",
            &["gen.text", "fx.halftone", "fx.color_adjustment"][..],
        ),
        (
            "preset.brik_07_kinetic_matrix",
            "Kinetic Matrix Poster",
            "Kinetic text arranged as a graphic matrix with glitch accents.",
            &["gen.kinetic", "fx.glitch", "fx.chromatic"][..],
        ),
        (
            "preset.brik_08_spectral_ascii",
            "Spectral ASCII Scan",
            "High-contrast field converted into a spectral terminal texture.",
            &["gen.texture_field", "fx.text_graphic", "fx.noise"][..],
        ),
        (
            "preset.brik_09_fun_grid",
            "Fun Grid Tool",
            "Playful animated grid with color modulation.",
            &["gen.grid", "fx.color_adjustment", "fx.glow"][..],
        ),
        (
            "preset.brik_10_dittertool",
            "TH_FR - DITTERTOOL TEST",
            "Dithered graphic field for a controlled poster texture.",
            &["gen.grid", "fx.dither", "fx.print"][..],
        ),
        (
            "preset.brik_11_split_it",
            "Split it",
            "Sliced and offset graphic composition with chromatic motion.",
            &["gen.pattern_grid", "fx.slice_shift", "fx.chromatic"][..],
        ),
        (
            "preset.brik_12_retro",
            "Retro",
            "Retro analog signal treatment with CRT and VHS character.",
            &["gen.mesh_gradient_fill", "fx.vhs", "fx.crt"][..],
        ),
        (
            "preset.brik_13_video_chat_slider",
            "Video Chat Slider",
            "Soft animated interface-like field with directional warping.",
            &["gen.gradient", "fx.warp", "fx.glow"][..],
        ),
        (
            "preset.brik_14_text_swarm",
            "Text Swarm",
            "Particle-like typography with luminous motion.",
            &["gen.text_particles", "fx.glow", "fx.bloom"][..],
        ),
        (
            "preset.brik_15_hover_crate_gallery",
            "Hover Crate Gallery",
            "Dimensional gallery surfaces with glass refraction.",
            &[
                "gen.mesh_gradient_fill",
                "fx.pattern_refraction",
                "fx.glass",
            ][..],
        ),
        (
            "preset.brik_16_png_to_3d_logo",
            "PNG to 3D Logo",
            "Raised logo-like typography with material shading.",
            &["gen.text_3d", "fx.materials", "fx.bloom"][..],
        ),
        (
            "preset.brik_17_pleat_fan",
            "Pleat Fan",
            "Radial pleat geometry with a controlled color wash.",
            &["gen.radial_rays", "fx.warp", "fx.color_adjustment"][..],
        ),
        (
            "preset.brik_18_flowing_text_trail",
            "Flowing Text Trail",
            "Text path motion with a glowing trail.",
            &["gen.text_path", "fx.glow", "fx.blur"][..],
        ),
        (
            "preset.brik_19_text_worm",
            "Text Worm",
            "Organic text motion with smear and chromatic detail.",
            &["gen.text_particles", "fx.smear", "fx.chromatic"][..],
        ),
        (
            "preset.brik_20_404_book",
            "404 Book",
            "Editorial type treatment with scanline distortion.",
            &["gen.text", "fx.crt", "fx.noise"][..],
        ),
        (
            "preset.brik_21_vision_boundary",
            "Vision Boundary Engine",
            "Outlined boundary field with a luminous edge treatment.",
            &["gen.texture_field", "fx.outlines", "fx.glow"][..],
        ),
        (
            "preset.brik_22_mikotone",
            "MikoTone",
            "Audio-reactive oscilloscope language with color shaping.",
            &["gen.oscilloscope", "fx.color_adjustment", "fx.bloom"][..],
        ),
        (
            "preset.brik_23_linetool",
            "linetool",
            "Line-based generative composition with geometric distortion.",
            &["gen.lsystem", "fx.geometry", "fx.chromatic"][..],
        ),
        (
            "preset.brik_24_arab_prisma",
            "Arab Prisma #1",
            "Prismatic geometric field with kaleidoscopic symmetry.",
            &["gen.kaleidoscope", "fx.chromatic", "fx.glow"][..],
        ),
        (
            "preset.brik_25_ambient_video_canvas",
            "Ambient Video Canvas",
            "Ambient cloud field for a soft continuously moving background.",
            &["gen.clouds", "fx.bloom", "fx.color_adjustment"][..],
        ),
    ];
    for (id, name, description, types) in brik {
        v.push(brik_recipe(id, name, description, types));
    }
    let gallery_offset = v.len();
    for (index, name) in BRIK_GALLERY_NAMES.iter().enumerate() {
        v.push(brik_library_recipe(gallery_offset + index, name));
    }
    let community_offset = v.len();
    for (index, name) in BRIK_COMMUNITY_NAMES.iter().enumerate() {
        let mut preset = brik_library_recipe(community_offset + index, name);
        preset.id = format!("preset.brik.community_{:04}", index + 1);
        preset.requirement_refs = vec!["BRIK-COMMUNITY".into(), "FX001".into()];
        preset.description = format!(
            "Brik Community tool concept: {name}. Native Flux graph with editable generator and effects."
        );
        v.push(preset);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_catalog;

    #[test]
    fn preset_types_exist_in_catalog() {
        let catalog = build_catalog();
        for p in all_presets() {
            for node in &p.nodes {
                assert!(
                    catalog.get(&node.type_id).is_some() || node.type_id == "flux.output",
                    "preset {} references missing type {}",
                    p.id,
                    node.type_id
                );
            }
            assert!(p.nodes.iter().any(|n| n.index == p.output));
        }
    }

    #[test]
    fn preset_count_at_least_required() {
        // EFFECTS 4 requires at least 35 named recipes.
        assert!(all_presets().len() >= 35);
    }

    #[test]
    fn brik_workspace_has_first_25_presets() {
        let presets = all_presets();
        let brik: Vec<_> = presets
            .iter()
            .filter(|p| {
                p.id.starts_with("preset.brik_")
                    && !p.id.contains("gallery_")
                    && !p.id.contains("community_")
            })
            .collect();
        assert_eq!(brik.len(), 25);
        assert!(brik
            .iter()
            .all(|p| !p.nodes.is_empty() && !p.edges.is_empty()));
    }

    #[test]
    fn brik_gallery_presets_are_native_and_unique() {
        let presets = all_presets();
        let gallery: Vec<_> = presets
            .iter()
            .filter(|p| p.id.contains("brik.gallery_"))
            .collect();
        assert_eq!(gallery.len(), BRIK_GALLERY_NAMES.len());
        let mut ids: Vec<_> = gallery.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), gallery.len());
        assert!(gallery
            .iter()
            .all(|p| p.nodes.iter().any(|n| n.type_id == "flux.output")));
    }

    #[test]
    fn brik_community_presets_are_native_and_unique() {
        let presets = all_presets();
        let community: Vec<_> = presets
            .iter()
            .filter(|p| p.id.contains("brik.community_"))
            .collect();
        assert_eq!(community.len(), BRIK_COMMUNITY_NAMES.len());
        let mut ids: Vec<_> = community.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), community.len());
        assert!(community
            .iter()
            .all(|p| p.nodes.iter().any(|n| n.type_id == "flux.output")));
    }
}
