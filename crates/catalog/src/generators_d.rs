//! 3D SDF / mesh / particle / physics / simulation / control generators.
//!
//! SDF (G080-G089) and mesh (G090-G107) rows run through the dedicated wgpu 3D
//! pass (`render/crates` scene pass, ported from the reference SDF renderer).
//! Particle, physics and fluid rows run through the CPU simulation workers
//! (fixed-step, deterministic); their manifests carry the real controls.

use crate::builders::*;
use crate::manifest::*;

fn scene_gen(
    id: &str,
    name: &str,
    category: &str,
    req: &str,
    time: TimeModel,
    help: &str,
    params: Vec<ParamDecl>,
) -> Manifest {
    let mut m = effect(id, name, category, "gen_scene", QualityClass::H, params, help, req);
    m.generator_id = id.to_string();
    m.time_model = time;
    m.outputs = vec![
        color_output(),
        PortDecl { id: "depth".into(), port_type: PortType::Depth, required: false },
        PortDecl { id: "normal".into(), port_type: PortType::Normal, required: false },
    ];
    m.inputs = vec![];
    m
}

/// Wrap a scene manifest with its row-ID ordering key and the scene shader
/// body. The shader for every 3D row is the shared scene renderer below.
fn scene_row(key: &'static str, m: Manifest) -> (&'static str, Manifest, &'static str) {
    (key, m, SCENE_BODY)
}

const SCENE_BODY: &str = r#"
// 3D SDF/mesh rows render through the dedicated wgpu scene pass, which writes
// this texture as its color output. The scene pass shares the working color
// and premultiplied-alpha convention, and provides depth/normal outputs from
// the same render.
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  return textureSampleLevel(src_tex, src_sampler, uv, 0.0);
}
"#;

pub fn sdf_rows() -> Vec<(&'static str, Manifest, &'static str)> {
    let mut v = Vec::new();
    v.push(scene_row("g080", scene_gen("gen.sdf_primitive", "SDF primitive", "3D SDF", "G080", TimeModel::Analytic,
        "Sphere, box, rounded box, capsule, cylinder, cone, torus, ellipsoid, plane or hex prism.",
        vec![p_enum("primitive", "Primitive", "sphere", &["sphere", "box", "roundedBox", "capsule", "cylinder", "cone", "torus", "ellipsoid", "plane", "hexPrism"]),
             p_float("size", "Size", 1.0, 0.05, 4.0, "m"), p_float("rounding", "Rounding", 0.1, 0.0, 1.0, "m")])));
    v.push(scene_row("g081", scene_gen("gen.sdf_blob", "Blob metaballs", "3D SDF", "G081", TimeModel::Analytic,
        "Smooth-union metaballs with orbit and audio deformation. The default scene is 4 spheres with breathing motion.",
        vec![p_int("count", "Count", 4, 1, 64), p_float("radius", "Radius", 0.5, 0.05, 2.0, "m"), p_float("merge", "Merge", 0.3, 0.0, 1.0, "m"),
             p_float("orbit", "Orbit speed", 0.3, 0.0, 4.0, ""), p_float("bassAmount", "Bass deformation", 0.5, 0.0, 2.0, "")])));
    v.push(scene_row("g082", scene_gen("gen.sdf_boolean", "SDF boolean", "3D SDF", "G082", TimeModel::Analytic,
        "Union, subtract, intersect and smooth variants with consistent signed-distance approximation.",
        vec![p_enum("op", "Operation", "smoothUnion", &["union", "subtract", "intersect", "smoothUnion", "smoothSubtract", "smoothIntersect"]), p_float("blend", "Blend radius", 0.2, 0.0, 1.0, "m")])));
    v.push(scene_row("g083", scene_gen("gen.sdf_domain", "SDF domain", "3D SDF", "G083", TimeModel::Analytic,
        "Translate/rotate/scale, repeat, mirror, twist, bend, taper, elongate and shell operators.",
        vec![p_enum("op", "Operator", "repeat", &["translate", "rotate", "scale", "repeat", "mirror", "twist", "bend", "taper", "elongate", "shell"]), p_float("amount", "Amount", 0.3, -2.0, 2.0, "")])));
    v.push(scene_row("g084", scene_gen("gen.sdf_noise", "SDF noise displacement", "3D SDF", "G084", TimeModel::Analytic,
        "Noise displacement with conservative step safety so marching never oversteps.",
        vec![p_float("amplitude", "Amplitude", 0.1, 0.0, 1.0, "m"), p_float("frequency", "Frequency", 2.0, 0.1, 32.0, ""), p_int("octaves", "Octaves", 3, 1, 8), p_float("speed", "Speed", 0.2, 0.0, 4.0, "")])));
    v.push(scene_row("g085", scene_gen("gen.sdf_gyroid", "Gyroid lattice", "3D SDF", "G085", TimeModel::Analytic,
        "Gyroid with threshold/thickness/scale, bounding volume and animated phase.",
        vec![p_float("scale", "Scale", 3.0, 0.5, 16.0, ""), p_float("thickness", "Thickness", 0.1, 0.01, 0.5, ""), p_float("phase", "Phase", 0.0, 0.0, 6.28, "")])));
    v.push(scene_row("g086", scene_gen("gen.sdf_fractal", "Fractal bulb", "3D SDF", "G086", TimeModel::Analytic,
        "Bounded Mandelbulb distance estimator with iteration cap and bailout.",
        vec![p_int("iterations", "Iterations", 12, 1, 32), p_float("power", "Power", 8.0, 2.0, 16.0, ""), p_float("bailout", "Bailout", 4.0, 1.5, 16.0, "")])));
    v.push(scene_row("g087", scene_gen("gen.sdf_render", "SDF render", "3D SDF", "G087", TimeModel::Analytic,
        "Raymarched renderer with color/Depth/Normal/Mask outputs, step budget and pixel-footprint epsilon.",
        vec![p_int("steps", "Ray steps", 96, 16, 256), p_float("maxDistance", "Max distance", 100.0, 1.0, 500.0, "m"), p_float("cameraDistance", "Camera distance", 3.5, 0.5, 50.0, "m")])));
    v.push(scene_row("g088", scene_gen("gen.sdf_material", "SDF material", "3D SDF", "G088", TimeModel::Analytic,
        "Diffuse, chrome, liquid metal, iridescent, emissive, translucent approximation and normal/debug shading.",
        vec![p_enum("material", "Material", "chrome", &["diffuse", "chrome", "liquidMetal", "iridescent", "emissive", "translucent", "normalDebug"]), p_float("roughness", "Roughness", 0.25, 0.0, 1.0, "")])));
    v.push(scene_row("g089", scene_gen("gen.sdf_mesh", "SDF to mesh", "3D SDF", "G089", TimeModel::Analytic,
        "Marching-cubes conversion within finite bounds, cancellable, producing Geometry and GLB export.",
        vec![p_int("resolution", "Resolution", 64, 8, 256), p_float("bounds", "Bounds", 2.0, 0.5, 10.0, "m")])));
    v
}

pub fn mesh_rows() -> Vec<(&'static str, Manifest, &'static str)> {
    let mut v = Vec::new();
    v.push(scene_row("g090", scene_gen("gen.mesh_primitive", "Mesh primitive", "3D Mesh", "G090", TimeModel::Analytic,
        "Box, sphere/icosphere, plane, disk, cylinder, cone, torus and capsule with segment limits.",
        vec![p_enum("primitive", "Primitive", "sphere", &["box", "sphere", "plane", "disk", "cylinder", "cone", "torus", "capsule"]), p_int("segments", "Segments", 32, 3, 256)])));
    v.push(scene_row("g091", scene_gen("gen.parametric", "Parametric surface", "3D Mesh", "G091", TimeModel::Analytic,
        "Torus knot, helix tube, Mobius strip and superquadric sampled UV meshes.",
        vec![p_enum("surface", "Surface", "torusKnot", &["torusKnot", "helix", "mobius", "superquadric"]), p_int("samples", "Samples", 128, 8, 512)])));
    v.push(scene_row("g092", scene_gen("gen.extrusion", "Path extrusion", "3D Mesh", "G092", TimeModel::Analytic,
        "Tube/ribbon along a path with parallel-transport frames, taper, twist and ribbon width.",
        vec![p_float("radius", "Radius", 0.2, 0.001, 2.0, "m"), p_float("twist", "Twist", 0.0, -6.28, 6.28, "rad"), p_float("taper", "Taper", 0.0, -1.0, 1.0, "")])));
    v.push(scene_row("g093", scene_gen("gen.deformer", "Mesh deformer", "3D Mesh", "G093", TimeModel::Analytic,
        "Bend, twist, taper, wave, noise, lattice and audio displacement with updated normals.",
        vec![p_enum("kind", "Kind", "bend", &["bend", "twist", "taper", "wave", "noise", "lattice", "audio"]), p_float("amount", "Amount", 0.3, -2.0, 2.0, "")])));
    v.push(scene_row("g094", scene_gen("gen.instancer", "Geometry instancer", "3D Mesh", "G094", TimeModel::Analytic,
        "Grid/radial/path/surface/image placements with checked instance count (max 1M).",
        vec![p_int("count", "Count", 256, 1, 1000000), p_enum("layout", "Layout", "grid", &["grid", "radial", "path", "surface", "image"])])));
    v.push(scene_row("g095", scene_gen("gen.terrain", "Terrain", "3D Mesh", "G095", TimeModel::Analytic,
        "Noise/image heightfield with grid 1282, displacement and height/slope coloring.",
        vec![p_int("grid", "Grid", 128, 8, 512), p_float("amplitude", "Amplitude", 1.0, 0.0, 10.0, "m"), p_float("scale", "Noise scale", 3.0, 0.1, 32.0, "")])));
    v.push(scene_row("g096", scene_gen("gen.voxel", "Voxel scene", "3D Mesh", "G096", TimeModel::Analytic,
        "Occupancy from noise/image/SDF as cubes or surface mesh with grid 64? default and memory cap.",
        vec![p_int("grid", "Grid", 64, 8, 128), p_float("threshold", "Threshold", 0.5, 0.0, 1.0, ""), p_enum("render", "Render", "cubes", &["cubes", "surface"])])));
    v.push(scene_row("g097", scene_gen("gen.point_cloud", "Point cloud", "3D Mesh", "G097", TimeModel::Analytic,
        "PLY positions/colors with point size, depth test and downsampling.",
        vec![p_float("pointSize", "Point size", 2.0, 0.5, 16.0, "px"), p_float("downsample", "Downsample", 1.0, 0.05, 1.0, "")])));
    v.push(scene_row("g098", scene_gen("gen.model", "Model scene", "3D Mesh", "G098", TimeModel::Analytic,
        "GLB/glTF import with PBR materials, animations, skin and morph targets. Unsupported extensions are listed.",
        vec![p_float("scale", "Scale", 1.0, 0.01, 100.0, ""), p_float("clip", "Animation clip", 0.0, 0.0, 64.0, "")])));
    v.push(scene_row("g099", scene_gen("gen.scene", "Scene composition", "3D Mesh", "G099", TimeModel::Analytic,
        "Merge scene objects/lights/cameras with namespaced IDs and render layer depth/mask outputs.",
        vec![p_bool("layers", "Render layers", true)])));
    v.push(scene_row("g100", scene_gen("gen.camera", "Camera", "3D Mesh", "G100", TimeModel::Analytic,
        "Perspective/orthographic/isometric with orbit, dolly, path, look-at, follow and deterministic shake.",
        vec![p_enum("kind", "Kind", "perspective", &["perspective", "orthographic", "isometric"]), p_float("fov", "FOV", 45.0, 5.0, 120.0, "deg"),
             p_float("orbit", "Orbit speed", 0.2, -4.0, 4.0, ""), p_float("shake", "Shake", 0.0, 0.0, 1.0, ""), p_float("near", "Near", 0.1, 0.01, 2.0, "m"), p_float("far", "Far", 100.0, 10.0, 1000.0, "m")])));
    v.push(scene_row("g101", scene_gen("gen.lights", "Lights", "3D Mesh", "G101", TimeModel::Analytic,
        "Directional, point, spot, hemisphere/ambient lights and environment lighting.",
        vec![p_float("key", "Key intensity", 1.0, 0.0, 8.0, ""), p_float("fill", "Fill intensity", 0.3, 0.0, 8.0, ""), p_float("ambient", "Ambient", 0.1, 0.0, 2.0, "")])));
    v.push(scene_row("g102", scene_gen("gen.material", "Material", "3D Mesh", "G102", TimeModel::Analytic,
        "PBR base color/metal/rough/emission/normal/alpha with toon and wireframe styles.",
        vec![p_color("color", "Base color", [0.8, 0.8, 0.85, 1.0]), p_float("metallic", "Metallic", 0.2, 0.0, 1.0, ""), p_float("roughness", "Roughness", 0.4, 0.0, 1.0, ""), p_enum("style", "Style", "pbr", &["pbr", "toon", "wireframe"])])));
    v.push(scene_row("g103", scene_gen("gen.scene_animation", "Scene animation", "3D Mesh", "G103", TimeModel::Analytic,
        "Keyframes, glTF clips, blend/crossfade, looping hierarchy transforms and camera paths.",
        vec![p_float("speed", "Speed", 1.0, 0.0, 4.0, "x"), p_float("blend", "Blend", 0.0, 0.0, 1.0, "")])));
    v.push(scene_row("g104", scene_gen("gen.architecture", "Procedural city", "3D Mesh", "G104", TimeModel::Analytic,
        "Repeatable building blocks, seeded skyline heights/windows, finite tiles.",
        vec![p_int("blocks", "Blocks", 64, 1, 2048), p_float("height", "Height", 2.0, 0.2, 20.0, "m")])));
    v.push(scene_row("g105", scene_gen("gen.planet", "Planet", "3D Mesh", "G105", TimeModel::Analytic,
        "Displaced spheres, rings, orbit fields and instancing with bounded coordinate precision.",
        vec![p_float("displacement", "Displacement", 0.1, 0.0, 1.0, ""), p_float("rings", "Rings", 0.0, 0.0, 1.0, ""), p_float("orbit", "Orbit speed", 0.1, 0.0, 4.0, "")])));
    v.push(scene_row("g106", scene_gen("gen.ocean", "Ocean", "3D Mesh", "G106", TimeModel::Analytic,
        "Sum-of-sines/Gerstner waves with foam approximation and sky reflection.",
        vec![p_float("amplitude", "Amplitude", 0.3, 0.0, 2.0, "m"), p_float("frequency", "Frequency", 1.0, 0.1, 8.0, ""), p_float("foam", "Foam", 0.2, 0.0, 1.0, "")])));
    v.push(scene_row("g107", scene_gen("gen.volume", "Volumetric field", "3D Mesh", "G107", TimeModel::Analytic,
        "Bounded density grid/noise raymarched as smoke/cloud/fire with slice and cost controls.",
        vec![p_enum("look", "Look", "smoke", &["smoke", "cloud", "fire"]), p_float("density", "Density", 0.5, 0.0, 1.0, ""), p_int("steps", "Steps", 48, 8, 128)])));
    v
}

/// Particle / physics / simulation / control rows: these are hosted by the
/// CPU fixed-step workers, not a fragment shader. Their manifests are complete
/// and they render through the shared presentation path.
fn cpu_node(id: &str, name: &str, category: &str, req: &str, time: TimeModel, help: &str, params: Vec<ParamDecl>) -> Manifest {
    let mut params2 = vec![p_float("wet", "Opacity", 1.0, 0.0, 1.0, "")];
    params2.extend(params);
    let mut m = effect(id, name, category, "cpu_node", QualityClass::H, params2, help, req);
    m.generator_id = id.to_string();
    m.time_model = time;
    m.inputs = vec![];
    m.outputs = vec![color_output()];
    m
}

fn cpu_row(key: &'static str, m: Manifest) -> (&'static str, Manifest, &'static str) {
    (key, m, CPU_PRESENT)
}

const CPU_PRESENT: &str = r#"
// CPU simulation nodes upload their state texture as `src_tex`; this presents
// it with the shared opacity.
fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32> {
  return textureSampleLevel(src_tex, src_sampler, uv, 0.0);
}
"#;

pub fn particle_rows() -> Vec<(&'static str, Manifest, &'static str)> {
    vec![
        cpu_row("g110", cpu_node("gen.particle_emitter", "Particle emitter", "Particles", "G110", TimeModel::FixedStep,
            "Point, line, circle, rectangle, sphere/volume, mesh, path, image or text emitters with rate/burst/seed.",
            vec![p_enum("emitter", "Emitter", "point", &["point", "line", "circle", "rectangle", "sphere", "mesh", "path", "image", "text"]),
                 p_float("rate", "Rate", 1000.0, 0.0, 20000.0, "/s"), p_float("burst", "Burst", 0.0, 0.0, 10000.0, ""), p_float("lifetime", "Lifetime", 3.0, 0.05, 30.0, "s")])),
        cpu_row("g111", cpu_node("gen.particle_init", "Particle initializer", "Particles", "G111", TimeModel::FixedStep,
            "Direction/cone/speed ranges and lifetime distribution, initial color/size/rotation.",
            vec![p_float("speed", "Speed", 1.0, 0.0, 10.0, "m/s"), p_float("cone", "Cone angle", 0.4, 0.0, 3.14, "rad"), p_float("size", "Size", 0.02, 0.001, 0.5, "m")])),
        cpu_row("g112", cpu_node("gen.particle_forces", "Particle forces", "Particles", "G112", TimeModel::FixedStep,
            "Gravity, wind, drag, curl noise, vector field, attract/repel, vortex/orbit and springs.",
            vec![p_float("gravity", "Gravity", 0.0, -30.0, 30.0, "m/s2"), p_float("wind", "Wind", 0.0, -30.0, 30.0, "m/s2"), p_float("drag", "Drag", 0.1, 0.0, 10.0, ""), p_float("curl", "Curl", 0.0, 0.0, 4.0, "")])),
        cpu_row("g113", cpu_node("gen.particle_overlife", "Over-life curves", "Particles", "G113", TimeModel::FixedStep,
            "Color/alpha/size/velocity/rotation curves sampled by normalized age.",
            vec![p_float("sizeEnd", "Size end", 0.0, 0.0, 2.0, ""), p_float("alphaEnd", "Alpha end", 0.0, 0.0, 1.0, "")])),
        cpu_row("g114", cpu_node("gen.particle_collision", "Particle collision", "Particles", "G114", TimeModel::FixedStep,
            "Plane/sphere/box/SDF collision with restitution, friction and kill/bounce/slide modes.",
            vec![p_enum("surface", "Surface", "plane", &["plane", "sphere", "box", "sdf"]), p_float("restitution", "Restitution", 0.5, 0.0, 1.0, ""), p_enum("response", "Response", "bounce", &["kill", "bounce", "slide"])])),
        cpu_row("g115", cpu_node("gen.particle_render", "Particle renderer", "Particles", "G115", TimeModel::FixedStep,
            "Points, billboard sprites and instanced meshes with alpha/additive and soft depth.",
            vec![p_enum("mode", "Mode", "points", &["points", "sprites", "mesh"]), p_enum("blend", "Blend", "alpha", &["alpha", "additive"]), p_float("size", "Size", 4.0, 0.5, 64.0, "px")])),
        cpu_row("g116", cpu_node("gen.particle_trails", "Particle trails", "Particles", "G116", TimeModel::FixedStep,
            "Bounded trail points per particle (default 16, max 128) with age fade.",
            vec![p_int("points", "Trail points", 16, 2, 128), p_float("fade", "Age fade", 0.5, 0.0, 1.0, "")])),
        cpu_row("g117", cpu_node("gen.particle_morph", "Particle morph", "Particles", "G117", TimeModel::FixedStep,
            "Image/text/geometry target points with stable per-particle-ID correspondence.",
            vec![p_float("blend", "Blend", 0.0, 0.0, 1.0, ""), p_int("count", "Count", 20000, 100, 200000)])),
        cpu_row("g118", cpu_node("gen.particle_boids", "Boids", "Particles", "G118", TimeModel::FixedStep,
            "Separation/alignment/cohesion with a bounded neighbor grid (max 32 neighbors).",
            vec![p_float("separation", "Separation", 1.0, 0.0, 4.0, ""), p_float("alignment", "Alignment", 0.5, 0.0, 4.0, ""), p_float("cohesion", "Cohesion", 0.5, 0.0, 4.0, "")])),
        cpu_row("g119", cpu_node("gen.particle_slime", "Agent slime", "Particles", "G119", TimeModel::FixedStep,
            "Agents deposit/turn in a field with diffusion/decay compute and bounded buffers.",
            vec![p_float("deposit", "Deposit", 0.3, 0.0, 2.0, ""), p_float("diffusion", "Diffusion", 0.1, 0.0, 0.5, ""), p_float("decay", "Decay", 0.02, 0.0, 0.5, "")])),
        cpu_row("g120", cpu_node("gen.particle_events", "Particle events", "Particles", "G120", TimeModel::FixedStep,
            "Birth/death/collision events with a per-frame cap of 1024 connectable to control nodes.",
            vec![p_bool("birth", "Birth events", false), p_bool("death", "Death events", false), p_bool("collision", "Collision events", false)])),
    ]
}

pub fn physics_rows() -> Vec<(&'static str, Manifest, &'static str)> {
    vec![
        cpu_row("g130", cpu_node("gen.rigid_world", "Rigid body world", "Physics", "G130", TimeModel::FixedStep,
            "Gravity, fixed dt, broadphase and deterministic reset/checkpoint.",
            vec![p_float("gravity", "Gravity", 9.81, 0.0, 30.0, "m/s2"), p_int("iterations", "Solver iterations", 4, 1, 16)])),
        cpu_row("g131", cpu_node("gen.rigid_body", "Body / collider", "Physics", "G131", TimeModel::FixedStep,
            "Fixed/dynamic/kinematic bodies with box/sphere/capsule/convex-hull colliders.",
            vec![p_enum("bodyType", "Body type", "dynamic", &["fixed", "dynamic", "kinematic"]), p_enum("collider", "Collider", "box", &["box", "sphere", "capsule", "convexHull"]), p_float("restitution", "Restitution", 0.4, 0.0, 1.0, "")])),
        cpu_row("g132", cpu_node("gen.joint_chain", "Joint chain", "Physics", "G132", TimeModel::FixedStep,
            "Fixed/hinge/rope/spring joints with chain generation from an anchor path.",
            vec![p_enum("joint", "Joint", "hinge", &["fixed", "hinge", "rope", "spring"]), p_int("links", "Links", 8, 1, 64)])),
        cpu_row("g133", cpu_node("gen.pendulum", "Pendulum", "Physics", "G133", TimeModel::FixedStep,
            "Single/double/array pendulums with length/mass/damping and event impulses.",
            vec![p_enum("kind", "Kind", "double", &["single", "double", "array"]), p_float("damping", "Damping", 0.01, 0.0, 0.5, "")])),
        cpu_row("g134", cpu_node("gen.physics_shards", "Physics shards", "Physics", "G134", TimeModel::FixedStep,
            "Precomputed Voronoi/grid fracture into convex pieces with mapped source UVs.",
            vec![p_int("pieces", "Pieces", 16, 4, 128), p_float("force", "Force", 0.5, 0.0, 4.0, "")])),
        cpu_row("g135", cpu_node("gen.cloth", "Cloth", "Physics", "G135", TimeModel::FixedStep,
            "PBD cloth grid 322 with structural/shear/bend constraints, pinned points, wind and collisions.",
            vec![p_int("grid", "Grid", 32, 4, 128), p_int("iterations", "Iterations", 6, 1, 32), p_float("wind", "Wind", 0.3, 0.0, 4.0, ""), p_bool("pinTop", "Pin top", true)])),
        cpu_row("g136", cpu_node("gen.jelly", "Jelly", "Physics", "G136", TimeModel::FixedStep,
            "Bounded spring/PBD soft mesh with volume-preservation approximation.",
            vec![p_float("stiffness", "Stiffness", 0.5, 0.0, 1.0, ""), p_float("damping", "Damping", 0.1, 0.0, 1.0, "")])),
        cpu_row("g137", cpu_node("gen.elastic", "Elastic motion", "Physics", "G137", TimeModel::FixedStep,
            "Critically damped springs for scalar/vector transforms with overshoot/bounce templates.",
            vec![p_float("stiffness", "Stiffness", 12.0, 0.1, 64.0, ""), p_float("damping", "Damping", 0.8, 0.05, 4.0, "")])),
    ]
}

pub fn simulation_rows() -> Vec<(&'static str, Manifest, &'static str)> {
    vec![
        cpu_row("g140", cpu_node("gen.fluid", "2D stable fluids", "Simulation", "G140", TimeModel::FixedStep,
            "Velocity/dye advection, divergence, 20 pressure-Jacobi iterations, projection and vorticity at 2562.",
            vec![p_int("grid", "Grid", 256, 64, 1024), p_float("viscosity", "Viscosity", 0.0, 0.0, 1.0, ""), p_float("dissipation", "Dissipation", 0.99, 0.9, 1.0, ""), p_int("pressureIterations", "Pressure iterations", 20, 4, 80)])),
        cpu_row("g141", cpu_node("gen.ink_fluid", "Ink fluid", "Simulation", "G141", TimeModel::FixedStep,
            "Ink/smoke palette and emitter templates on the shared stable-fluids solver.",
            vec![p_color("inkColor", "Ink color", [0.9, 0.8, 0.2, 1.0]), p_float("viscosity", "Viscosity", 0.05, 0.0, 1.0, ""), p_int("emitters", "Emitters", 2, 1, 8)])),
        cpu_row("g142", cpu_node("gen.shallow_water", "Ripple tank", "Simulation", "G142", TimeModel::FixedStep,
            "Height/velocity grid 2562 with CFL-limited dt, damping and impulse radius/strength.",
            vec![p_int("grid", "Grid", 256, 64, 1024), p_float("damping", "Damping", 0.02, 0.0, 0.2, ""), p_float("impulse", "Impulse strength", 0.3, 0.0, 2.0, "")])),
        cpu_row("g143", cpu_node("gen.reaction_brush", "Reaction brush", "Simulation", "G143", TimeModel::FixedStep,
            "Reaction-diffusion with brush/image seeding, palette and evolving height outputs.",
            vec![p_float("feed", "Feed", 0.0367, 0.0, 0.1, ""), p_float("kill", "Kill", 0.0649, 0.0, 0.1, ""), p_int("grid", "Grid", 256, 64, 512)])),
        cpu_row("g144", cpu_node("gen.sand", "Sand fall", "Simulation", "G144", TimeModel::FixedStep,
            "Integer grid 2562 with alternating deterministic phases for sand/water/wall cells.",
            vec![p_int("grid", "Grid", 256, 64, 1024), p_float("sandRatio", "Sand ratio", 0.3, 0.0, 1.0, "")])),
        cpu_row("g145", cpu_node("gen.sph", "Particle fluid", "Simulation", "G145", TimeModel::FixedStep,
            "Bounded SPH/PBD fluid with 10k default particles, spatial grid and surface rendering.",
            vec![p_int("particles", "Particles", 10000, 100, 100000), p_float("stiffness", "Stiffness", 0.5, 0.0, 1.0, ""), p_int("iterations", "Iterations", 3, 1, 8)])),
        cpu_row("g146", cpu_node("gen.em_field", "Magnetic field", "Simulation", "G146", TimeModel::Analytic,
            "Point charge/magnet vector field with streamlines and particle tracing.",
            vec![p_int("charges", "Charges", 3, 1, 16), p_float("strength", "Strength", 1.0, 0.0, 10.0, "")])),
        cpu_row("g147", cpu_node("gen.wave_interference", "Wave interference", "Simulation", "G147", TimeModel::Analytic,
            "Multiple emitters with frequency/phase/amplitude, distance attenuation and beat sync.",
            vec![p_int("emitters", "Emitters", 3, 1, 16), p_float("frequency", "Frequency", 1.0, 0.1, 8.0, "")])),
        cpu_row("g148", cpu_node("gen.volume_smoke", "Volume smoke", "Simulation", "G148", TimeModel::FixedStep,
            "Stylized bounded 64? density-advection grid; the procedural mode is clearly labeled.",
            vec![p_enum("mode", "Mode", "procedural", &["procedural", "solver"]), p_int("grid", "Grid", 64, 32, 128), p_float("buoyancy", "Buoyancy", 0.3, 0.0, 2.0, "")])),
    ]
}

pub fn control_rows() -> Vec<(&'static str, Manifest, &'static str)> {
    vec![
        cpu_row("g150", cpu_node("gen.audio_visual", "Audio spectrum", "Control", "G150", TimeModel::Analytic,
            "Spectrum/waveform/spectrogram of a selected bus with bin scale, smoothing and colors, plus raw feature outputs.",
            vec![p_enum("kind", "Kind", "bars", &["bars", "wave", "spectrogram"]), p_int("bins", "Bins", 64, 8, 512), p_float("smoothing", "Smoothing", 0.5, 0.0, 1.0, "")])),
        cpu_row("g151", cpu_node("gen.rhythm", "Rhythm", "Control", "G151", TimeModel::Analytic,
            "Internal tempo with 16-step Euclidean patterns, seeded probability, subdivisions and swing.",
            vec![p_float("tempo", "Tempo", 120.0, 30.0, 300.0, "bpm"), p_int("steps", "Steps", 16, 2, 64), p_int("pulses", "Euclidean pulses", 5, 1, 64), p_float("swing", "Swing", 0.0, 0.0, 0.6, "")])),
        cpu_row("g152", cpu_node("gen.oscillators", "Oscillator bank", "Control", "G152", TimeModel::Analytic,
            "LFO shapes, phase offsets and additive waves with bounded stable integration.",
            vec![p_int("count", "Count", 4, 1, 32), p_float("frequency", "Frequency", 0.5, 0.01, 20.0, "Hz"), p_enum("shape", "Shape", "sine", &["sine", "triangle", "saw", "square"])])),
        cpu_row("g153", cpu_node("gen.envelope", "Envelope", "Control", "G153", TimeModel::Analytic,
            "ADSR/envelope follower with retrigger/legato, event delay, sample-and-hold and random-on-beat.",
            vec![p_float("attack", "Attack", 5.0, 0.0, 2000.0, "ms"), p_float("release", "Release", 120.0, 0.0, 4000.0, "ms")])),
        cpu_row("g154", cpu_node("gen.midi_source", "MIDI visual", "Control", "G154", TimeModel::Analytic,
            "Note grid, piano-roll trails and chord constellations from live/recorded MIDI.",
            vec![p_float("trail", "Trail seconds", 1.0, 0.0, 8.0, "s"), p_bool("mpe", "MPE dimensions", true)])),
        cpu_row("g155", cpu_node("gen.osc_source", "OSC value", "Control", "G155", TimeModel::Analytic,
            "OSC learn, address/type, stale policy, smoothing and timestamped recorded take.",
            vec![p_float("smoothing", "Smoothing", 0.1, 0.0, 1.0, ""), p_float("staleTimeout", "Stale timeout", 0.5, 0.05, 5.0, "s")])),
        cpu_row("g156", cpu_node("gen.csv_source", "Data chart", "Control", "G156", TimeModel::Analytic,
            "CSV/JSON columns with time mapping, deterministic interpolation and chart layout.",
            vec![p_float("interpolation", "Interpolation", 1.0, 0.0, 1.0, ""), p_bool("loop", "Loop", true)])),
        cpu_row("g157", cpu_node("gen.network_source", "Network data", "Control", "G157", TimeModel::External,
            "WebSocket/HTTP polling with validated bounded schema, recording and offline replay. User-enabled only.",
            vec![p_float("timeout", "Timeout", 5.0, 0.5, 60.0, "s"), p_int("maxBytes", "Max bytes", 65536, 1024, 1048576)])),
        cpu_row("g158", cpu_node("gen.interaction", "Interaction", "Control", "G158", TimeModel::External,
            "Mouse/tablet pressure/keyboard/gamepad in normalized coordinates with record/loop/replay.",
            vec![p_float("smoothing", "Smoothing", 0.0, 0.0, 1.0, "")])),
        cpu_row("g159", cpu_node("gen.clock", "Clock / metadata", "Control", "G159", TimeModel::Analytic,
            "Project time, UTC anchor, filename or marker values with deterministic export substitution.",
            vec![p_enum("source", "Source", "projectTime", &["projectTime", "utc", "filename", "marker"])])),
        cpu_row("g160", cpu_node("gen.file_sequencer", "File sequencer", "Control", "G160", TimeModel::Analytic,
            "Folder image selection by beat/index/random seed with explicit permissions and a stable snapshot list.",
            vec![p_enum("select", "Select", "beat", &["beat", "index", "random"]), p_bool("sorted", "Sorted list", true)])),
        cpu_row("g161", cpu_node("gen.event_sequencer", "Event sequencer", "Control", "G161", TimeModel::Analytic,
            "Scene recall and control macros on finite timed cues with quantized next-beat/bar transitions.",
            vec![p_int("steps", "Steps", 8, 1, 64), p_enum("quantize", "Quantize", "bar", &["beat", "bar", "none"])])),
        cpu_row("g162", cpu_node("gen.control_math", "Control math", "Control", "G162", TimeModel::Analytic,
            "Remap, clamp, smooth, lag, invert, quantize, gate, compare, sum/product and expression AST (bounded depth).",
            vec![p_float("inMin", "In min", 0.0, -10.0, 10.0, ""), p_float("inMax", "In max", 1.0, -10.0, 10.0, ""), p_float("outMin", "Out min", 0.0, -10.0, 10.0, ""), p_float("outMax", "Out max", 1.0, -10.0, 10.0, ""), p_enum("op", "Operator", "remap", &["remap", "clamp", "smooth", "invert", "quantize", "gate"])])),
    ]
}

pub fn conditional_rows() -> Vec<(&'static str, Manifest, &'static str)> {
    let cond = |id: &str, name: &str, req: &str, help: &str| -> (&'static str, Manifest, &'static str) {
        let mut m = effect(id, name, "Conditional", "conditional", QualityClass::H,
            vec![p_float("wet", "Opacity", 1.0, 0.0, 1.0, "")], help, req);
        m.generator_id = id.to_string();
        m.time_model = TimeModel::External;
        m.inputs = vec![source_port()];
        m.outputs = vec![color_output()];
        m.help = format!("{help} Requires an external model/runtime; the node stays editable and reports MODEL_MISSING when absent.");
        ("conditional", m, CPU_PRESENT)
    };
    vec![
        cond("gen.optical_flow_model", "Optical flow analysis", "G170", "Frame-pair flow and confidence from an OpenCV/native adapter (upgraded path). The required non-neural baseline is G064."),
        cond("gen.landmarks", "Face/hand/pose", "G171", "Landmarks with normalized positions and confidence; attaches emitters/cameras."),
        cond("gen.segmentation", "Segmentation", "G172", "Person/foreground mask with temporal smoothing."),
        cond("gen.depth_model", "Monocular depth", "G173", "Relative/metric depth with holes and confidence."),
        cond("gen.tracking", "Object tracking", "G174", "Tracked transform/points attached to text or scenes."),
        cond("gen.motion_heatmap", "Motion heatmap", "G175", "Webcam frame-difference motion field (non-neural baseline available)."),
        cond("gen.gaussian_splat", "Gaussian splat", "G176", "Gaussian splat scene importer/runtime adapter with byte/count limits."),
        cond("gen.neural_animation", "Neural animation", "G177", "Externally generated image/video/latent outputs become normal assets/takes."),
        cond("gen.custom_wgsl", "Custom WGSL", "G180", "User-supplied WGSL generator with validated bounded resources and a stop action."),
        cond("gen.p5_sketch", "p5/Canvas sketch", "G181", "Isolated browser worker with renderFrame(time) virtual clock and recorded capture."),
        cond("gen.three_scene", "Three.js scene", "G182", "Isolated renderer with explicit frame bytes; deterministic stepping for export."),
        cond("gen.lottie_runtime", "Lottie/Rive adapter", "G183", "Native Lottie is G043; other runtime formats require an actual parser/frame API."),
        cond("gen.python_source", "Python generator", "G184", "User-enabled executable/config with bounded RGBA pipe and timeout/crash recovery."),
        cond("gen.external_feed", "External app feed", "G185", "Existing screen/Spout/Syphon/NDI route with frame/time metadata and recording."),
    ]
}
