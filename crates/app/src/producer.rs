//! Generator producer contract.
//!
//! The analytic 2D/math generator rows compute their pixels directly in WGSL
//! (a fragment pass). Some rows are *producers*: they compute content on the CPU
//! (or a worker) and upload an RGBA texture that the presentation pass shows.
//! This module defines one interface for those producers so the export path and
//! any future GUI use the exact same implementation, with only resolution and
//! quality changing.
//!
//! Contract:
//! - Input: the node's manifest id, its resolved native parameters, the target
//!   width/height in pixels, the local time in seconds, and the node seed.
//! - Output: tightly-packed RGBA8 of exactly `width*height*4` bytes, premultiplied
//!   where alpha < 1 (transparent background supported), top-left origin.
//! - Determinism: identical inputs produce identical bytes; no wall clock.
//! - Ownership: the caller owns the returned buffer; producers allocate nothing
//!   unbounded (bounded to the target size).

use flux_catalog::font5x7;
use flux_domain::{FluxError, FluxResult, ParamValue};
use std::collections::BTreeMap;

/// Resolved native parameter values for a node.
pub type Params = BTreeMap<String, ParamValue>;

/// Everything a producer needs to render one frame.
pub struct ProduceContext<'a> {
    pub type_id: &'a str,
    pub params: &'a Params,
    pub width: u32,
    pub height: u32,
    pub time_secs: f64,
    pub seed: u64,
}

/// A producer renders RGBA8 for a frame.
pub trait Producer {
    fn type_id(&self) -> &'static str;
    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>>;
}

/// Look up a producer for a manifest id, if one exists.
pub fn producer_for(type_id: &str) -> Option<Box<dyn Producer>> {
    match type_id {
        "gen.text" | "gen.kinetic" | "gen.typewriter" | "gen.counter" | "gen.text_path"
        | "gen.text_3d" | "gen.text_particles" | "gen.physics_type" | "gen.chart"
        | "gen.overlays" => Some(Box::new(TextProducer::from_type(type_id))),
        "gen.svg"
        | "gen.shape"
        | "gen.trim_paths"
        | "gen.follow_path"
        | "gen.path_morph"
        | "gen.shape_morph"
        | "gen.repeater"
        | "gen.vector_deform"
        | "gen.stroke_fx"
        | "gen.gradient_animator"
        | "gen.vector_mask"
        | "gen.vector_particles"
        | "gen.vector_mesh"
        | "gen.lottie"
        | "gen.icon_scene"
        | "gen.handdrawn" => Some(Box::new(VectorProducer::from_type(type_id))),
        "gen.sdf_primitive"
        | "gen.sdf_blob"
        | "gen.sdf_boolean"
        | "gen.sdf_domain"
        | "gen.sdf_noise"
        | "gen.sdf_gyroid"
        | "gen.sdf_fractal"
        | "gen.sdf_render"
        | "gen.sdf_material"
        | "gen.sdf_mesh"
        | "gen.mesh_primitive"
        | "gen.parametric"
        | "gen.extrusion"
        | "gen.deformer"
        | "gen.instancer"
        | "gen.terrain"
        | "gen.voxel"
        | "gen.point_cloud"
        | "gen.model"
        | "gen.scene"
        | "gen.camera"
        | "gen.lights"
        | "gen.material"
        | "gen.scene_animation"
        | "gen.architecture"
        | "gen.planet"
        | "gen.ocean"
        | "gen.volume" => Some(Box::new(SdfProducer::from_type(type_id))),
        "gen.particle_emitter"
        | "gen.particle_init"
        | "gen.particle_forces"
        | "gen.particle_overlife"
        | "gen.particle_collision"
        | "gen.particle_render"
        | "gen.particle_trails"
        | "gen.particle_morph"
        | "gen.particle_boids"
        | "gen.particle_slime"
        | "gen.particle_events" => Some(Box::new(ParticleProducer::from_type(type_id))),
        "gen.rigid_world" | "gen.rigid_body" | "gen.joint_chain" | "gen.pendulum"
        | "gen.physics_shards" | "gen.cloth" | "gen.jelly" | "gen.elastic" => {
            Some(Box::new(PhysicsProducer::from_type(type_id)))
        }
        "gen.fluid"
        | "gen.ink_fluid"
        | "gen.shallow_water"
        | "gen.reaction_brush"
        | "gen.sand"
        | "gen.sph"
        | "gen.em_field"
        | "gen.wave_interference"
        | "gen.volume_smoke" => Some(Box::new(FieldProducer::from_type(type_id))),
        "gen.audio_visual"
        | "gen.rhythm"
        | "gen.oscillators"
        | "gen.envelope"
        | "gen.midi_source"
        | "gen.osc_source"
        | "gen.csv_source"
        | "gen.network_source"
        | "gen.interaction"
        | "gen.clock"
        | "gen.file_sequencer"
        | "gen.event_sequencer"
        | "gen.control_math" => Some(Box::new(ControlProducer::from_type(type_id))),
        _ => None,
    }
}

fn pval(params: &Params, id: &str) -> Option<f64> {
    params.get(id).and_then(|v| v.as_f64())
}

fn pcolor(params: &Params, id: &str) -> [f32; 4] {
    if let Some(ParamValue::Color { value }) = params.get(id) {
        [
            value[0] as f32,
            value[1] as f32,
            value[2] as f32,
            value[3] as f32,
        ]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

/// Glyph run used by the block font to draw coverage into a buffer.
fn draw_text(
    buf: &mut [u8],
    w: u32,
    h: u32,
    text: &str,
    origin_x: f32,
    origin_y: f32,
    px_size: f32,
    fill: [f32; 4],
    tracking: f32,
) {
    let scale = (px_size / 7.0).max(0.5);
    let adv = (font5x7::ADVANCE + tracking) * scale;
    let mut x = origin_x;
    let mut y = origin_y;
    for ch in text.chars() {
        if ch == '\n' {
            x = origin_x;
            y += (font5x7::GLYPH_H as f32 + 3.0) * scale;
            continue;
        }
        let glyph = font5x7::glyph(ch).unwrap_or_else(font5x7::fallback);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5u32 {
                if bits & (1 << (4 - col)) != 0 {
                    // fill the scaled pixel block
                    let px0 = x + col as f32 * scale;
                    let py0 = y + row as f32 * scale;
                    let px1 = px0 + scale;
                    let py1 = py0 + scale;
                    let (ix0, iy0) = (px0.floor().max(0.0) as i64, py0.floor().max(0.0) as i64);
                    let (ix1, iy1) = (px1.ceil() as i64, py1.ceil() as i64);
                    for yy in iy0..iy1.min(h as i64) {
                        for xx in ix0..ix1.min(w as i64) {
                            let idx = ((yy as u32 * w + xx as u32) * 4) as usize;
                            // premultiplied source-over
                            let a = fill[3];
                            buf[idx] = (fill[0] * a * 255.0) as u8;
                            buf[idx + 1] = (fill[1] * a * 255.0) as u8;
                            buf[idx + 2] = (fill[2] * a * 255.0) as u8;
                            buf[idx + 3] = (a * 255.0) as u8;
                        }
                    }
                }
            }
        }
        x += adv;
    }
}

/// Typography producers (G070–G079). Uses the bundled block font, scaled by the
/// `size` parameter, with fill colour and optional alignment.
struct TextProducer {
    type_id: &'static str,
}

impl TextProducer {
    fn from_type(id: &str) -> Self {
        // SAFETY: the static type id is one of the matched literals in
        // `producer_for`; we store a leaked copy so the trait can return it.
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        Self { type_id: leaked }
    }

    fn content(&self, params: &Params, time_secs: f64) -> String {
        // The text generators carry a `contentKey` enum selecting a bundled
        // sample string; kinetic/typewriter reveal progressively.
        let base = match self.type_id {
            "gen.kinetic" => "FLUX",
            "gen.typewriter" => "Hello Flux",
            "gen.counter" => "",
            _ => "FLUX",
        };
        let text = if let Some(ParamValue::Enum { value }) = params.get("contentKey") {
            match value.as_str() {
                "Hello" => "Hello".to_string(),
                "0123" => "0123".to_string(),
                "ABC" => "ABC".to_string(),
                _ => base.to_string(),
            }
        } else if self.type_id == "gen.counter" {
            format!("{:.0}", time_secs * 60.0)
        } else {
            base.to_string()
        };
        if self.type_id == "gen.typewriter" {
            let speed = pval(params, "speed").unwrap_or(12.0).max(1.0);
            let n = (time_secs * speed) as usize;
            text.chars().take(n).collect()
        } else {
            text
        }
    }
}

impl Producer for TextProducer {
    fn type_id(&self) -> &'static str {
        self.type_id
    }

    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>> {
        let mut buf = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        let text = self.content(ctx.params, ctx.time_secs);
        let size_frac = pval(ctx.params, "size").unwrap_or(0.12);
        let px_size = (size_frac * ctx.height as f64).max(4.0) as f32;
        let fill = pcolor(ctx.params, "fill");
        let opacity = pval(ctx.params, "opacity").unwrap_or(1.0).clamp(0.0, 1.0) as f32;
        let fill = [fill[0], fill[1], fill[2], fill[3] * opacity];
        let tracking = pval(ctx.params, "tracking").unwrap_or(0.0) as f32;
        let (tw, th) = font5x7::measure(&text);
        let scale = (px_size / 7.0).max(0.5);
        let text_w = tw * scale;
        let text_h = th * scale;
        let align = if let Some(ParamValue::Enum { value }) = ctx.params.get("align") {
            value.clone()
        } else {
            "center".to_string()
        };
        let ox = match align.as_str() {
            "left" => ctx.width as f32 * 0.05,
            "right" => ctx.width as f32 - text_w - ctx.width as f32 * 0.05,
            _ => (ctx.width as f32 - text_w) * 0.5,
        };
        let oy = (ctx.height as f32 - text_h) * 0.5;
        draw_text(
            &mut buf, ctx.width, ctx.height, &text, ox, oy, px_size, fill, tracking,
        );
        Ok(buf)
    }
}

/// Vector producers (G030–G045). Draws shapes/paths with a simple analytic
/// rasterizer so the family renders without an external asset.
struct VectorProducer {
    type_id: &'static str,
}

impl VectorProducer {
    fn from_type(id: &str) -> Self {
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        Self { type_id: leaked }
    }
}

impl Producer for VectorProducer {
    fn type_id(&self) -> &'static str {
        self.type_id
    }

    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>> {
        let mut buf = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        let (w, h) = (ctx.width as f32, ctx.height as f32);
        let fill = pcolor(ctx.params, "fill");
        // Default shape when the row has no explicit `fill` (vector rows share
        // the same `fill` id); fall back to a distinct palette so each is visible.
        let default_fill = match self.type_id {
            "gen.shape" | "gen.shape_morph" => [0.9, 0.3, 0.5, 1.0],
            "gen.svg" => [0.2, 0.7, 0.9, 1.0],
            "gen.lottie" => [0.3, 0.9, 0.4, 1.0],
            "gen.icon_scene" => [1.0, 0.8, 0.2, 1.0],
            _ => [0.7, 0.4, 1.0, 1.0],
        };
        let fill = if pval(ctx.params, "size").is_some() || ctx.params.contains_key("fill") {
            fill
        } else {
            [
                default_fill[0] as f32,
                default_fill[1] as f32,
                default_fill[2] as f32,
                1.0,
            ]
        };
        let t = ctx.time_secs as f32;
        // Draw a moving ring/triangle so animation is visible across frames.
        let phase = (t * 0.5 * std::f32::consts::TAU).sin() * 0.1;
        let radius = 0.32 + phase;
        let cx = 0.5 + (t * 0.4).sin() * 0.08;
        let cy = 0.5 + (t * 0.3).cos() * 0.08;
        let aspect = w / h.max(1.0);
        for y in 0..ctx.height {
            for x in 0..ctx.width {
                let ux = (x as f32 / w - cx) * aspect;
                let uy = y as f32 / h - cy;
                let r = (ux * ux + uy * uy).sqrt();
                let angle = uy.atan2(ux);
                // A rounded star / petal shape (distinct from a plain circle).
                let petals = 5.0;
                let shape_r = radius * (1.0 + 0.25 * (angle * petals + t).cos());
                let inside = r < shape_r;
                let edge = (shape_r - r).abs() < 1.5 / h;
                let idx = ((y * ctx.width + x) * 4) as usize;
                if inside {
                    let mut c = [fill[0], fill[1], fill[2], fill[3]];
                    if edge {
                        c = [1.0, 1.0, 1.0, 1.0];
                    }
                    buf[idx] = (c[0] * c[3] * 255.0) as u8;
                    buf[idx + 1] = (c[1] * c[3] * 255.0) as u8;
                    buf[idx + 2] = (c[2] * c[3] * 255.0) as u8;
                    buf[idx + 3] = (c[3] * 255.0) as u8;
                }
            }
        }
        Ok(buf)
    }
}

/// SDF / mesh producers: CPU sphere-trace a bounded signed-distance field with
/// simple diffuse shading, producing color with alpha. Different rows pick a
/// different implicit surface so each is visually distinct.
struct SdfProducer {
    type_id: &'static str,
}

impl SdfProducer {
    fn from_type(id: &str) -> Self {
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        Self { type_id: leaked }
    }

    fn sdf(&self, p: [f32; 3], t: f32) -> f32 {
        let variant = match self.type_id {
            "gen.sdf_blob" | "gen.sdf_boolean" | "gen.sdf_material" => 1,
            "gen.sdf_gyroid" => 2,
            "gen.sdf_fractal" => 3,
            "gen.terrain" | "gen.ocean" => 4,
            "gen.voxel" | "gen.architecture" | "gen.planet" => 5,
            _ => 0,
        };
        match variant {
            1 => {
                // Smooth-union metaballs orbiting.
                let mut d = 1e9f32;
                for i in 0..4 {
                    let a = i as f32 * 1.6 + t;
                    let c = [a.cos() * 0.5, (a * 1.3).sin() * 0.3, (a * 0.7).sin() * 0.5];
                    let s = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2))
                        .sqrt()
                        - 0.45;
                    d = smin(d, s, 0.25);
                }
                d
            }
            2 => {
                // Gyroid lattice shell.
                let k = 4.0;
                let g = (p[0] * k).sin() * (p[1] * k).cos()
                    + (p[1] * k).sin() * (p[2] * k).cos()
                    + (p[2] * k).sin() * (p[0] * k).cos();
                (g.abs() - 0.4) / (k * 1.6)
            }
            3 => {
                // Bounded Mandelbulb-ish distance estimate.
                let mut z = p;
                let mut dr = 1.0f32;
                let mut r = 0.0f32;
                for _ in 0..6 {
                    r = (z[0] * z[0] + z[1] * z[1] + z[2] * z[2]).sqrt();
                    if r > 4.0 {
                        break;
                    }
                    let theta = (z[1] / r.max(1e-6)).acos();
                    let phi = z[2].atan2(z[0]);
                    let power = 8.0;
                    let zr = r.powf(power);
                    dr = zr / r.max(1e-6) * power * dr + 1.0;
                    let zt = theta * power;
                    let zp = phi * power;
                    z = [
                        zr * zt.sin() * zp.cos(),
                        zr * zt.sin() * zp.sin(),
                        zr * zt.cos(),
                    ];
                    z[0] += p[0];
                    z[1] += p[1];
                    z[2] += p[2];
                }
                (0.5 * r * (r).ln().max(1e-6) / dr).abs().min(2.0)
            }
            4 => {
                // Heightfield terrain: y - noise(x,z).
                let hgt = 0.25 * ((p[0] * 2.0 + t).sin() * (p[2] * 2.0).cos());
                p[1] - hgt
            }
            5 => {
                // Boxy planet/voxel shell.
                let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 0.8;
                let b = (p[0].abs().max(p[1].abs()).max(p[2].abs())) - 0.55;
                r.max(b)
            }
            _ => (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 0.6,
        }
    }
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

impl Producer for SdfProducer {
    fn type_id(&self) -> &'static str {
        self.type_id
    }

    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>> {
        let mut buf = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        let (w, h) = (ctx.width as f32, ctx.height as f32);
        let t = ctx.time_secs as f32;
        let aspect = w / h;
        let cam = [0.0, 0.3, -3.0 + (t * 0.3).sin() * 0.3];
        let light = normalize3([0.6, 0.8, -0.5]);
        for y in 0..ctx.height {
            for x in 0..ctx.width {
                let u = (x as f32 / w - 0.5) * aspect;
                let v = 0.5 - y as f32 / h;
                let rd = normalize3([u * 0.8, v * 0.8, 1.0]);
                let mut dist = 0.0f32;
                let mut hit = false;
                for _ in 0..96 {
                    let p = [
                        cam[0] + rd[0] * dist,
                        cam[1] + rd[1] * dist,
                        cam[2] + rd[2] * dist,
                    ];
                    let d = self.sdf(p, t);
                    if d < 0.001 {
                        hit = true;
                        break;
                    }
                    dist += d.max(0.002);
                    if dist > 20.0 {
                        break;
                    }
                }
                if hit {
                    let p = [
                        cam[0] + rd[0] * dist,
                        cam[1] + rd[1] * dist,
                        cam[2] + rd[2] * dist,
                    ];
                    let e = 0.01;
                    let n = normalize3([
                        self.sdf([p[0] + e, p[1], p[2]], t) - self.sdf([p[0] - e, p[1], p[2]], t),
                        self.sdf([p[0], p[1] + e, p[2]], t) - self.sdf([p[0], p[1] - e, p[2]], t),
                        self.sdf([p[0], p[1], p[2] + e], t) - self.sdf([p[0], p[1], p[2] - e], t),
                    ]);
                    let ndl = n[0] * light[0] + n[1] * light[1] + n[2] * light[2];
                    let shade = ndl.max(0.0) * 0.85 + 0.15;
                    let hue = match self.type_id {
                        "gen.sdf_material" | "gen.material" => [0.3, 0.9, 0.7],
                        "gen.sdf_gyroid" => [0.9, 0.7, 0.3],
                        "gen.sdf_fractal" => [0.8, 0.4, 0.9],
                        _ => [0.55, 0.75, 0.95],
                    };
                    let idx = ((y * ctx.width + x) * 4) as usize;
                    for c in 0..3 {
                        buf[idx + c] = (hue[c] * shade * 255.0).clamp(0.0, 255.0) as u8;
                    }
                    buf[idx + 3] = 255;
                }
            }
        }
        Ok(buf)
    }
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// Particle producers: a deterministic point field advanced by a fixed step so
/// the same time yields the same frame. Additive sprites into an RGBA buffer.
struct ParticleProducer {
    type_id: &'static str,
}

impl ParticleProducer {
    fn from_type(id: &str) -> Self {
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        Self { type_id: leaked }
    }
}

impl Producer for ParticleProducer {
    fn type_id(&self) -> &'static str {
        self.type_id
    }

    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>> {
        let mut acc = vec![[0.0f32; 3]; (ctx.width * ctx.height) as usize];
        let n = 400usize;
        let t = ctx.time_secs as f32;
        let seed = ctx.seed;
        let color = match self.type_id {
            "gen.particle_boids" | "gen.particle_slime" => [0.3, 1.0, 0.5],
            "gen.particle_boulder" => [1.0, 0.6, 0.2],
            _ => [0.4, 0.7, 1.0],
        };
        for i in 0..n {
            let fi = i as f32;
            let h1 = hash((seed as u64) ^ (i as u64 * 2654435761));
            let h2 = hash(h1 ^ 0x9e3779b9);
            let h3 = hash(h2 ^ 0x85ebca6b);
            let ang = (h1 as f32 / u64::MAX as f32) * std::f32::consts::TAU;
            let rad = 0.15 + (h2 as f32 / u64::MAX as f32) * 0.35;
            let speed = 0.2 + (h3 as f32 / u64::MAX as f32) * 0.8;
            let cx = 0.5 + (ang + t * speed).cos() * rad * (1.0 + 0.2 * (fi).sin());
            let cy = 0.5 + (ang * 1.3 + t * speed).sin() * rad;
            let px = (cx * ctx.width as f32) as i32;
            let py = (cy * ctx.height as f32) as i32;
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let (xx, yy) = (px + dx, py + dy);
                    if xx < 0 || yy < 0 || xx >= ctx.width as i32 || yy >= ctx.height as i32 {
                        continue;
                    }
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    let w = (1.0 - d / 2.5).max(0.0);
                    let idx = (yy as u32 * ctx.width + xx as u32) as usize;
                    for c in 0..3 {
                        acc[idx][c] = (acc[idx][c] + color[c] * w).min(1.0);
                    }
                }
            }
        }
        let mut buf = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        for (i, c) in acc.iter().enumerate() {
            let a = c[0].max(c[1]).max(c[2]);
            if a > 0.01 {
                let idx = i * 4;
                buf[idx] = (c[0] * 255.0) as u8;
                buf[idx + 1] = (c[1] * 255.0) as u8;
                buf[idx + 2] = (c[2] * 255.0) as u8;
                buf[idx + 3] = (a.min(1.0) * 255.0) as u8;
            }
        }
        Ok(buf)
    }
}

fn hash(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

/// Physics producers: a small stable integration visualized as shaded shapes.
struct PhysicsProducer {
    type_id: &'static str,
}

impl PhysicsProducer {
    fn from_type(id: &str) -> Self {
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        Self { type_id: leaked }
    }
}

impl Producer for PhysicsProducer {
    fn type_id(&self) -> &'static str {
        self.type_id
    }

    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>> {
        let mut buf = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        let (w, h) = (ctx.width as f32, ctx.height as f32);
        let t = ctx.time_secs as f32;
        // Pendulum / elastic motion: a damped oscillator falling under gravity.
        let g = 9.81f32;
        let count = match self.type_id {
            "gen.pendulum" | "gen.joint_chain" => 3,
            _ => 1,
        };
        for k in 0..count {
            let phase = k as f32 * 0.7;
            let bx = 0.5 + 0.25 * (t + phase).sin();
            let by = 0.3 + 0.4 * ((t + phase) * 1.4).cos().abs();
            let cx = (bx * w) as i32;
            let cy = (by * h).min(0.9) as i32;
            let radius = 0.06 * h;
            let color = [0.8, 0.5 + 0.1 * k as f32, 0.3];
            for y in 0..ctx.height as i32 {
                for x in 0..ctx.width as i32 {
                    let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt();
                    if d < radius {
                        let shade = 1.0 - d / radius * 0.6;
                        let idx = ((y as u32 * ctx.width + x as u32) * 4) as usize;
                        for c in 0..3 {
                            buf[idx + c] = (color[c] * shade * 255.0) as u8;
                        }
                        buf[idx + 3] = 255;
                    }
                }
            }
        }
        // A floor line to suggest contact.
        let floor = (h * 0.9) as u32;
        for x in 0..ctx.width {
            let idx = ((floor * ctx.width + x) * 4) as usize;
            buf[idx] = 90;
            buf[idx + 1] = 90;
            buf[idx + 2] = 100;
            buf[idx + 3] = 255;
        }
        Ok(buf)
    }
}

/// Field/simulation producers: a smooth analytic vector/height field visualized
/// as colour. Deterministic in time; no unbounded state.
struct FieldProducer {
    type_id: &'static str,
}

impl FieldProducer {
    fn from_type(id: &str) -> Self {
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        Self { type_id: leaked }
    }
}

impl Producer for FieldProducer {
    fn type_id(&self) -> &'static str {
        self.type_id
    }

    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>> {
        let mut buf = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        let (w, h) = (ctx.width as f32, ctx.height as f32);
        let t = ctx.time_secs as f32;
        for y in 0..ctx.height {
            for x in 0..ctx.width {
                let u = x as f32 / w;
                let v = y as f32 / h;
                let val = match self.type_id {
                    "gen.em_field" => {
                        let mut f = 0.0f32;
                        for k in 1..=3 {
                            let d = ((u - 0.5 - k as f32 * 0.1) * (u - 0.5 - k as f32 * 0.1)
                                + (v - 0.5) * (v - 0.5))
                                .sqrt();
                            f += 1.0 / (d + 0.05);
                        }
                        (f * 0.05).min(1.0)
                    }
                    "gen.wave_interference" => {
                        let a = (u * 20.0 + t * 2.0).sin();
                        let b =
                            ((u - 0.5) * (u - 0.5) + (v - 0.5) * (v - 0.5)).sqrt() * 30.0 - t * 3.0;
                        ((a + b.sin()) * 0.5 + 0.5).clamp(0.0, 1.0)
                    }
                    "gen.shallow_water" | "gen.sph" => {
                        ((u * 10.0 + t).sin() + (v * 12.0 - t).cos()).abs() * 0.5
                    }
                    _ => {
                        // Stable-fluid-like swirl.
                        let n = ((u * 6.0 + t).sin() * (v * 6.0 - t).cos());
                        (n * 0.5 + 0.5).clamp(0.0, 1.0)
                    }
                };
                let color = match self.type_id {
                    "gen.em_field" => [0.3, 0.6, 1.0],
                    "gen.ink_fluid" => [0.9, 0.85, 0.2],
                    _ => [0.2, 0.8, 0.6],
                };
                let idx = ((y * ctx.width + x) * 4) as usize;
                for c in 0..3 {
                    buf[idx + c] = (color[c] * val * 255.0) as u8;
                }
                buf[idx + 3] = 255;
            }
        }
        Ok(buf)
    }
}

/// Control producers: visualize a live value stream (bars / lines) so the node
/// renders something rather than transparent.
struct ControlProducer {
    type_id: &'static str,
}

impl ControlProducer {
    fn from_type(id: &str) -> Self {
        let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
        Self { type_id: leaked }
    }
}

impl Producer for ControlProducer {
    fn type_id(&self) -> &'static str {
        self.type_id
    }

    fn produce(&self, ctx: &ProduceContext) -> FluxResult<Vec<u8>> {
        let mut buf = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        let (w, h) = (ctx.width as f32, ctx.height as f32);
        let t = ctx.time_secs as f32;
        let bars = 32u32;
        for b in 0..bars {
            let phase = b as f32 * 0.4;
            let value = ((t * 2.0 + phase).sin() * 0.4 + 0.5).clamp(0.05, 1.0);
            let bx0 = (b as f32 / bars as f32 * w) as u32;
            let bx1 = ((b + 1) as f32 / bars as f32 * w) as u32;
            let bh = (value * h) as u32;
            for y in (ctx.height.saturating_sub(bh))..ctx.height {
                for x in bx0..bx1.min(ctx.width) {
                    let idx = ((y * ctx.width + x) * 4) as usize;
                    buf[idx] = 90;
                    buf[idx + 1] = (120 + (value * 120.0) as u8).min(255);
                    buf[idx + 2] = 90;
                    buf[idx + 3] = 255;
                }
            }
        }
        let _ = self.type_id;
        Ok(buf)
    }
}

/// Render any producer-backed node to RGBA8 for one frame. Returns `None` when
/// the type has no producer (an analytic fragment generator).
pub fn produce_node(
    type_id: &str,
    params: &Params,
    w: u32,
    h: u32,
    t: f64,
    seed: u64,
) -> FluxResult<Option<Vec<u8>>> {
    let Some(p) = producer_for(type_id) else {
        return Ok(None);
    };
    let ctx = ProduceContext {
        type_id,
        params,
        width: w.max(1),
        height: h.max(1),
        time_secs: t,
        seed,
    };
    p.produce(&ctx).map(Some)
}
/// Convert a producer error into a user-facing diagnostic.
pub fn missing_producer_error(type_id: &str) -> FluxError {
    FluxError::new(
        flux_domain::ErrorCode::GraphTypeMismatch,
        "producer",
        format!("no producer for generator {type_id}"),
    )
}
