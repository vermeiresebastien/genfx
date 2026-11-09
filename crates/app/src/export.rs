//! Offline export: renders a project's timeline through the authoritative GPU
//! graph, one exact frame at a time, and writes PNG+WAV then encodes a video.
//!
//! The live editor and the exporter share the same `RuntimeGraph`, frame
//! context, and parameter resolution. Export uses an isolated runtime so it
//! cannot disturb the live simulation.

use flux_catalog::{build_catalog, Catalog};
use flux_domain::{ClipSource, FluxError, FluxResult, Project, RationalTime};
use flux_engine::frame::{export_frame_count, frame_time, FrameContext};
use flux_engine::router::Router;
use flux_engine::timeline::render_items;
use flux_engine::RenderItem;
use flux_media::{EncodeCodec, MediaWorker};
use flux_render::graph_exec::RuntimeGraph;
use flux_render::pipeline::{EffectCompiler, NodeParamsBuffer};
use flux_render::preview::read_full;
use flux_render::{BackendPref, Gpu};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Progress callback: (frame_index, total_frames).
pub type ProgressFn = Box<dyn Fn(u64, u64) + Send>;

pub struct ExportOptions {
    pub out_dir: PathBuf,
    pub project_name: String,
    pub codec: EncodeCodec,
    pub width: u32,
    pub height: u32,
    pub range: Option<(RationalTime, RationalTime)>,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct ExportResult {
    pub frames: u64,
    pub samples: usize,
    pub frames_dir: PathBuf,
    pub wav: Option<PathBuf>,
    pub video: Option<PathBuf>,
}

/// Render the whole timeline at project resolution into a PNG sequence + WAV and
/// optionally encode an MP4. Returns exact counts.
pub fn export_project(
    project: &Project,
    opts: &ExportOptions,
    progress: Option<ProgressFn>,
) -> FluxResult<ExportResult> {
    let gpu = Gpu::new(BackendPref::Auto)?;
    let catalog = build_catalog();
    let mut compiler = EffectCompiler::new(&gpu);

    let fps = project.settings.frame_rate;
    let (start, duration) = match opts.range {
        Some((s, d)) => (s, d),
        None => (
            project.timeline.range.start,
            project.timeline.range.duration,
        ),
    };
    let total = export_frame_count(duration, fps);

    let frames_dir = opts
        .out_dir
        .join(format!("{}_frames", sanitize(&opts.project_name)));
    std::fs::create_dir_all(&frames_dir)?;

    // Determine which graph to render: the master graph if present, else build a
    // presentation graph that composites the visible clip's own graph or a solid
    // fallback. For each frame we compile a per-frame runtime lazily (cached by
    // graph id) so we do not rebuild pipelines.
    let mut roots_cache: HashMap<String, RuntimeGraph> = HashMap::new();
    let mut router = Router::default();
    let media_worker = MediaWorker::new().ok();

    let mut rendered = 0u64;
    for i in 0..total {
        if opts.cancel.load(Ordering::Relaxed) {
            return Err(FluxError::new(
                flux_domain::ErrorCode::MediaDecode,
                "export",
                "cancelled",
            ));
        }
        let t = start
            .add(frame_time(i, fps)?)
            .map_err(FluxError::new_internal)?;
        let frame_ctx = FrameContext::for_project(project, t, i);
        let px = render_single_frame(
            &gpu,
            &catalog,
            &mut compiler,
            project,
            &frame_ctx,
            &mut roots_cache,
            &mut router,
            media_worker.as_ref(),
            opts.width,
            opts.height,
        )?;
        let path = frames_dir.join(format!("frame_{:06}.png", i + 1));
        flux_media::write_png_rgba(&path, opts.width, opts.height, &px)?;
        rendered += 1;
        if let Some(cb) = &progress {
            cb(i + 1, total);
        }
    }

    // Audio: deterministic stereo mix of the range.
    let samples = mix_audio(project, start, duration)?;
    let wav_path = opts
        .out_dir
        .join(format!("{}.wav", sanitize(&opts.project_name)));
    flux_media::write_wav_stereo(
        &wav_path,
        project.settings.audio.sample_rate,
        &samples.samples,
    )?;

    // Encode video if requested and supported.
    let mut video = None;
    if !matches!(opts.codec, EncodeCodec::None) {
        if let Ok(worker) = MediaWorker::new() {
            let fps_str = format!("{}/{}", fps.num, fps.den);
            let out = opts
                .out_dir
                .join(format!("{}.mp4", sanitize(&opts.project_name)));
            let res = if matches!(opts.codec, EncodeCodec::Gif) {
                worker.encode_video_from_frames(
                    &frames_dir,
                    &fps_str,
                    None,
                    &out.with_extension("gif"),
                    EncodeCodec::Gif,
                )
            } else {
                worker.encode_video_from_frames(
                    &frames_dir,
                    &fps_str,
                    Some(&wav_path),
                    &out,
                    opts.codec,
                )
            };
            if res.is_ok() {
                video = Some(if matches!(opts.codec, EncodeCodec::Gif) {
                    out.with_extension("gif")
                } else {
                    out
                });
            }
        }
    }

    Ok(ExportResult {
        frames: rendered,
        samples: samples.samples.len(),
        frames_dir,
        wav: Some(wav_path),
        video,
    })
}

fn render_single_frame(
    gpu: &Gpu,
    catalog: &Catalog,
    compiler: &mut EffectCompiler,
    project: &Project,
    ctx_in: &FrameContext,
    roots_cache: &mut HashMap<String, RuntimeGraph>,
    router: &mut Router,
    worker: Option<&MediaWorker>,
    width: u32,
    height: u32,
) -> FluxResult<Vec<u8>> {
    // The render targets are at the export resolution, so the frame context's
    // dimensions must match them (otherwise full-frame source sampling is wrong).
    let mut ctx = ctx_in.clone();
    ctx.output_width = width;
    ctx.output_height = height;
    ctx.source_width = width;
    ctx.source_height = height;
    let ctx = &ctx;
    // Build the display graph: pick the top-most visible clip and render its
    // clip graph (or a solid generator). This is the composition path the live
    // editor uses; multi-track compositing is handled by graph scope nodes.
    let items = render_items(project, ctx.project_time);
    let chosen_graph_id = items.iter().find_map(|it| match it {
        RenderItem::Clip { clip, .. } => {
            // Prefer a clip graph if present; otherwise synthesize a graph from
            // the clip's generator/asset.
            clip_graph_for(project, clip)
        }
        _ => None,
    });

    let graph = if let Some(gid) = chosen_graph_id {
        if let Some(stripped) = gid.strip_prefix("generator:") {
            Some(generator_graph(stripped))
        } else if gid == "asset" {
            Some(asset_passthrough_graph())
        } else {
            project.graphs.get(&gid).cloned()
        }
    } else {
        None
    };

    // Fallback: a solid color plate graph (transparent) — a well-defined image
    // when no clip is present, never a panic.
    let graph = match graph {
        Some(g) => g,
        None => solid_fallback(),
    };

    let key = graph.id.clone();
    if !roots_cache.contains_key(&key) {
        let rt = RuntimeGraph::activate(gpu, compiler, catalog, &graph, width, height, 1)?;
        roots_cache.insert(key.clone(), rt);
    }
    let rt = roots_cache.get_mut(&key).unwrap();

    // Resolve parameters for every node in the graph.
    let mut resolved: HashMap<String, NodeParamsBuffer> = HashMap::new();
    let snapshot = flux_domain::params::FeatureSnapshot::default();
    for pass in &rt.compiled.passes {
        let entry = catalog.get(&pass.type_id);
        let mut buf = match entry {
            Some(e) => NodeParamsBuffer::defaults(&e.manifest),
            None => NodeParamsBuffer {
                floats: vec![[0.0; 4]],
                uints: vec![[0u32; 4]],
            },
        };
        if let Some(e) = entry {
            let values = router.resolve_node(
                &graph,
                &pass.node_id,
                ctx.project_time,
                &snapshot,
                ctx.sim_dt,
                project,
            )?;
            let (_, layout) = flux_catalog::pack::params_wgsl(&e.manifest.parameters);
            for slot in &layout.slots {
                if let Some(v) = values.get(&slot.id) {
                    match slot.kind {
                        flux_catalog::pack::SlotKind::F32 => {
                            buf.floats[slot.vec4_index][slot.component] = *v as f32
                        }
                        flux_catalog::pack::SlotKind::U32 => {
                            buf.uints[slot.vec4_index][slot.component] = (*v).max(0.0) as u32
                        }
                        _ => {}
                    }
                }
            }
        }
        resolved.insert(pass.node_id.clone(), buf);
    }

    // Source texture: decode the chosen asset clip's frame at its exact source
    // time; otherwise transparent (generator clips ignore the source).
    let mut source_pixels: Option<Vec<u8>> = None;
    if let Some(worker) = worker {
        for it in &items {
            if let RenderItem::Clip { clip, .. } = it {
                if let flux_engine::timeline::ResolvedSource::Asset {
                    asset_id,
                    source_time,
                } = &clip.source
                {
                    if let Some(asset) = project.assets.get(asset_id) {
                        let cancel = Arc::new(AtomicBool::new(false));
                        if let Ok(frame) = worker.decode_rgba_frame(
                            std::path::Path::new(&asset.original_path),
                            *source_time,
                            width,
                            height,
                            cancel,
                        ) {
                            source_pixels = Some(frame.rgba);
                            break;
                        }
                    }
                }
            }
        }
    }
    let src_pixels = source_pixels.unwrap_or_else(|| vec![0u8; (width * height * 4) as usize]);
    let src_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("export-src"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &src_tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &src_pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let src_view = src_tex.create_view(&Default::default());

    // Produce content for producer-backed nodes (vector/text/etc.) using the
    // same contract the GUI uses. Textures are kept alive for the execute call.
    let mut producer_textures: Vec<(wgpu::Texture, wgpu::TextureView)> = Vec::new();
    let mut producer_views: HashMap<String, wgpu::TextureView> = HashMap::new();
    for pass in &rt.compiled.passes {
        if let Some(e) = catalog.get(&pass.type_id) {
            let values = router.resolve_node(
                &graph,
                &pass.node_id,
                ctx.project_time,
                &snapshot,
                ctx.sim_dt,
                project,
            )?;
            // Start from manifest defaults + the node's stored params so colours,
            // enums and vectors keep their true values; float params use the
            // resolved (keyframed/modulated) value.
            let mut params_map: BTreeMap<String, flux_domain::ParamValue> = BTreeMap::new();
            let node = graph.node(&pass.node_id);
            for p in &e.manifest.parameters {
                let mut pv = param_value_from_json(&p.default);
                if let Some(n) = node.and_then(|n| n.params.get(&p.id)) {
                    pv = n.clone();
                }
                if let Some(v) = values.get(&p.id) {
                    if matches!(p.kind, flux_catalog::ParamKind::Float) {
                        pv = flux_domain::ParamValue::Float { value: *v };
                    }
                }
                params_map.insert(p.id.clone(), pv);
            }
            let t = ctx.project_time.as_f64();
            if let Some(rgba) = crate::producer::produce_node(
                &pass.type_id,
                &params_map,
                width,
                height,
                t,
                ctx.project_seed,
            )? {
                let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("producer"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                let view = tex.create_view(&Default::default());
                producer_views.insert(pass.node_id.clone(), view.clone());
                producer_textures.push((tex, view));
            }
        }
    }

    rt.execute_with_producers(
        gpu,
        compiler,
        catalog,
        ctx,
        &src_view,
        &resolved,
        1.0,
        &producer_views,
    )?;

    // Read at full resolution.
    let (_w, _h, px) = read_full(gpu, &rt.target_texture(), width, height)?;
    let _ = producer_textures;
    Ok(px)
}

/// Convert a manifest default JSON value into a `ParamValue`.
fn param_value_from_json(v: &serde_json::Value) -> flux_domain::ParamValue {
    use flux_domain::ParamValue;
    if let Some(f) = v.as_f64() {
        return ParamValue::Float { value: f };
    }
    if let Some(b) = v.as_bool() {
        return ParamValue::Bool { value: b };
    }
    if let Some(s) = v.as_str() {
        return ParamValue::Enum {
            value: s.to_string(),
        };
    }
    if let Some(a) = v.as_array() {
        let nums: Vec<f64> = a.iter().filter_map(|x| x.as_f64()).collect();
        return match nums.len() {
            4 => ParamValue::Color {
                value: [nums[0], nums[1], nums[2], nums[3]],
            },
            3 => ParamValue::Vec3 {
                value: [nums[0], nums[1], nums[2]],
            },
            2 => ParamValue::Vec2 {
                value: [nums[0], nums[1]],
            },
            _ => ParamValue::Float { value: 0.0 },
        };
    }
    ParamValue::Float { value: 0.0 }
}

fn clip_graph_for(project: &Project, clip: &flux_engine::VisibleClip) -> Option<String> {
    // The VisibleClip does not carry graph_id; look up the project clip.
    if let Some(c) = project.timeline.clips.get(&clip.clip_id) {
        if let Some(gid) = &c.graph_id {
            return Some(gid.clone());
        }
        match &c.source {
            ClipSource::GeneratorScene { generator_id } => {
                return Some(format!("generator:{}", generator_id))
            }
            ClipSource::Asset { .. } => return Some("asset".into()),
            _ => {}
        }
    }
    None
}

/// A graph that presents the decoded source texture unchanged.
pub fn asset_passthrough_graph() -> flux_domain::Graph {
    use flux_domain::{Graph, GraphEdge, GraphNode};
    use std::collections::BTreeMap;
    Graph {
        id: "export.asset".into(),
        name: "asset".into(),
        nodes: vec![
            GraphNode {
                id: "src".into(),
                type_id: "flux.source".into(),
                type_version: 1,
                position: [0.0, 0.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            },
            GraphNode {
                id: "out".into(),
                type_id: "flux.output".into(),
                type_version: 1,
                position: [240.0, 0.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            },
        ],
        edges: vec![GraphEdge {
            from_node: "src".into(),
            from_port: "color".into(),
            to_node: "out".into(),
            to_port: "source".into(),
        }],
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    }
}

/// A transparent solid plate used when no clip is at the frame.
pub fn solid_fallback() -> flux_domain::Graph {
    use flux_domain::{Graph, GraphEdge, GraphNode};
    use std::collections::BTreeMap;
    let mut params = BTreeMap::new();
    params.insert(
        "color".to_string(),
        flux_domain::ParamValue::Color {
            value: [0.0, 0.0, 0.0, 0.0],
        },
    );
    Graph {
        id: "export.solid".into(),
        name: "solid".into(),
        nodes: vec![
            GraphNode {
                id: "solid".into(),
                type_id: "gen.solid".into(),
                type_version: 1,
                position: [0.0, 0.0],
                params,
                bypass: false,
                missing_type: false,
            },
            GraphNode {
                id: "out".into(),
                type_id: "flux.output".into(),
                type_version: 1,
                position: [220.0, 0.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            },
        ],
        edges: vec![GraphEdge {
            from_node: "solid".into(),
            from_port: "color".into(),
            to_node: "out".into(),
            to_port: "source".into(),
        }],
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    }
}

/// Build a generator graph for a generator id (gen.solid etc.).
pub fn generator_graph(generator_id: &str) -> flux_domain::Graph {
    use flux_domain::{Graph, GraphEdge, GraphNode};
    use std::collections::BTreeMap;
    Graph {
        id: format!("graph:{}", generator_id),
        name: generator_id.into(),
        nodes: vec![
            GraphNode {
                id: "gen".into(),
                type_id: generator_id.into(),
                type_version: 1,
                position: [0.0, 0.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            },
            GraphNode {
                id: "out".into(),
                type_id: "flux.output".into(),
                type_version: 1,
                position: [220.0, 0.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            },
        ],
        edges: vec![GraphEdge {
            from_node: "gen".into(),
            from_port: "color".into(),
            to_node: "out".into(),
            to_port: "source".into(),
        }],
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    }
}

/// Deterministic stereo mix of a project range. Audio clips are decoded from
/// their asset and placed at their timeline positions with gain/pan/fades.
fn mix_audio(
    project: &Project,
    start: RationalTime,
    duration: RationalTime,
) -> FluxResult<MixedAudio> {
    let sr = project.settings.audio.sample_rate;
    let one_sample = RationalTime::new(1, sr).unwrap();
    let total_samples = duration.div_round(one_sample).max(0) as usize;
    let mut out = vec![0.0f32; total_samples * 2];
    let mixer = flux_audio::Mixer::new(sr);
    let worker = MediaWorker::new().ok();

    for track in project.timeline.ordered_tracks() {
        if track.kind != flux_domain::TrackKind::Audio || track.muted {
            continue;
        }
        for clip in project.timeline.track_clips(&track.id) {
            let ClipSource::Asset { asset_id } = &clip.source else {
                continue;
            };
            let Some(asset) = project.assets.get(asset_id) else {
                continue;
            };
            let Some(worker) = &worker else { continue };
            let track_audio =
                match worker.decode_audio_stereo(std::path::Path::new(&asset.original_path), sr) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
            let clip_start_off = clip.range.start.sub(start).unwrap_or(RationalTime::ZERO);
            let offset_samples = if clip_start_off.is_negative() {
                0
            } else {
                clip_start_off.div_round(one_sample) as usize
            };
            let src_off = clip.source_range.start.div_round(one_sample) as usize * 2;
            let n = ((clip.range.duration.div_round(one_sample)) as usize * 2)
                .min(track_audio.samples.len());
            let mut seg = vec![0.0f32; n];
            for i in 0..n {
                if src_off + i < track_audio.samples.len() {
                    seg[i] = track_audio.samples[src_off + i];
                }
            }
            if !clip.audio.fade_in.is_zero() || !clip.audio.fade_out.is_zero() {
                let fi = clip.audio.fade_in.div_round(one_sample) as usize * 2;
                let fo = clip.audio.fade_out.div_round(one_sample) as usize * 2;
                mixer.apply_fade(&mut seg, fi, fo);
            }
            let target = &mut out[offset_samples * 2..];
            mixer.mix_stereo(
                target,
                &seg,
                2,
                clip.audio.gain_db + track.gain_db,
                clip.audio.pan,
            );
        }
    }
    Ok(MixedAudio { samples: out })
}

pub struct MixedAudio {
    pub samples: Vec<f32>,
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

trait FluxErrorInternal {
    fn new_internal(e: flux_domain::time::RationalError) -> FluxError;
}

impl FluxErrorInternal for FluxError {
    fn new_internal(e: flux_domain::time::RationalError) -> FluxError {
        FluxError::new(
            flux_domain::ErrorCode::InvalidCommand,
            "time",
            e.to_string(),
        )
    }
}

/// Render a contact sheet: each requested effect applied to gen.plasma, tiled
/// into a single RGBA image with `cells_x` columns. Returns (w, h, rgba).
pub fn contact_sheet(
    effects: &[String],
    cell_w: u32,
    cell_h: u32,
    cells_x: u32,
) -> FluxResult<(u32, u32, Vec<u8>)> {
    let gpu = Gpu::new(BackendPref::Auto)?;
    let catalog = build_catalog();
    let mut compiler = EffectCompiler::new(&gpu);
    let rows = (effects.len() as u32).div_ceil(cells_x.max(1));
    let out_w = cells_x * cell_w;
    let out_h = rows * cell_h;
    let mut sheet = vec![0u8; (out_w * out_h * 4) as usize];
    let mut cache: HashMap<String, RuntimeGraph> = HashMap::new();
    let mut router = Router::default();
    let project = flux_domain::Project::new("sheet", 3);
    let ctx = FrameContext::for_project(&project, RationalTime::new(1, 2).unwrap(), 0);

    for (i, eff) in effects.iter().enumerate() {
        let graph = crate::samples::chain_graph("gen.plasma", &[eff.as_str()]);
        let key = graph.id.clone();
        if !cache.contains_key(&key) {
            let rt =
                RuntimeGraph::activate(&gpu, &mut compiler, &catalog, &graph, cell_w, cell_h, 1)?;
            cache.insert(key.clone(), rt);
        }
        let rt = cache.get_mut(&key).unwrap();
        let mut resolved = HashMap::new();
        let snap = flux_domain::params::FeatureSnapshot::default();
        for pass in &rt.compiled.passes {
            if let Some(e) = catalog.get(&pass.type_id) {
                let mut buf = NodeParamsBuffer::defaults(&e.manifest);
                let vals = router.resolve_node(
                    &graph,
                    &pass.node_id,
                    ctx.project_time,
                    &snap,
                    ctx.sim_dt,
                    &project,
                )?;
                let (_, layout) = flux_catalog::pack::params_wgsl(&e.manifest.parameters);
                for slot in &layout.slots {
                    if let Some(v) = vals.get(&slot.id) {
                        match slot.kind {
                            flux_catalog::pack::SlotKind::F32 => {
                                buf.floats[slot.vec4_index][slot.component] = *v as f32
                            }
                            flux_catalog::pack::SlotKind::U32 => {
                                buf.uints[slot.vec4_index][slot.component] = (*v).max(0.0) as u32
                            }
                            _ => {}
                        }
                    }
                }
                resolved.insert(pass.node_id.clone(), buf);
            }
        }
        let src = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: cell_w,
                height: cell_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sv = src.create_view(&Default::default());
        rt.execute(&gpu, &compiler, &catalog, &ctx, &sv, &resolved, 1.0)?;
        let (_, _, px) = read_full(&gpu, &rt.target_texture(), cell_w, cell_h)?;
        // blit into sheet
        let cx = (i as u32 % cells_x) * cell_w;
        let cy = (i as u32 / cells_x) * cell_h;
        for y in 0..cell_h {
            let dst = (((cy + y) * out_w + cx) * 4) as usize;
            let s = ((y * cell_w) * 4) as usize;
            sheet[dst..dst + (cell_w * 4) as usize]
                .copy_from_slice(&px[s..s + (cell_w * 4) as usize]);
        }
    }
    Ok((out_w, out_h, sheet))
}

/// Convenience: render with the default encode codec.
pub fn export_default(project: &Project, out_dir: &Path) -> FluxResult<ExportResult> {
    export_project(
        project,
        &ExportOptions {
            out_dir: out_dir.to_path_buf(),
            project_name: project.name.clone(),
            codec: EncodeCodec::H264,
            width: project.settings.size.width,
            height: project.settings.size.height,
            range: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        None,
    )
}
