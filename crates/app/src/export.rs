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
    /// Which class of audio evidence fed the rendered frames. A deterministic
    /// export always records `DocumentOnly`; `Live` cannot be produced by this
    /// path at all, and naming the source here is what makes that checkable
    /// after the fact rather than a comment in the code.
    pub feature_source: flux_domain::params::FeatureSource,
    /// Hops of analysed media audio the export actually had to work from.
    /// Zero with a nonzero `samples` means the project carried no audio.
    pub audio_windows: usize,
}

/// Map the UI's format token to an encoder. Unknown tokens fall back to H.264 so
/// export always yields a video by default.
pub fn codec_from_name(name: &str) -> EncodeCodec {
    match name.trim().to_ascii_lowercase().as_str() {
        "hevc" | "h265" => EncodeCodec::Hevc,
        "prores" | "mov" => EncodeCodec::ProRes,
        "gif" => EncodeCodec::Gif,
        "jpeg" | "jpg" => EncodeCodec::JpegSequence,
        "png" | "sequence" | "none" => EncodeCodec::None,
        _ => EncodeCodec::H264,
    }
}

/// Per-frame preview callback: (done, total, rgba, width, height). Lets the GUI
/// show the actual export output while it renders.
pub type FrameFn = Box<dyn Fn(u64, u64, &[u8], u32, u32) + Send>;

/// Render the whole timeline at project resolution into a PNG sequence + WAV and
/// optionally encode an MP4. Returns exact counts.
pub fn export_project(
    project: &Project,
    opts: &ExportOptions,
    progress: Option<ProgressFn>,
) -> FluxResult<ExportResult> {
    export_project_impl(project, opts, progress, None)
}

/// Same as [`export_project`], additionally invoking `frame_cb` with each rendered
/// frame for a live preview.
pub fn export_project_with_frames(
    project: &Project,
    opts: &ExportOptions,
    progress: Option<ProgressFn>,
    frame_cb: Option<FrameFn>,
) -> FluxResult<ExportResult> {
    export_project_impl(project, opts, progress, frame_cb)
}

fn export_project_impl(
    project: &Project,
    opts: &ExportOptions,
    progress: Option<ProgressFn>,
    frame_cb: Option<FrameFn>,
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

    // PNG frames are only written when the user explicitly asked for a sequence.
    // Video codecs stream raw RGBA straight into ffmpeg instead.
    let write_png_frames = matches!(opts.codec, EncodeCodec::None | EncodeCodec::JpegSequence);
    let frames_dir = opts
        .out_dir
        .join(format!("{}_frames", sanitize(&opts.project_name)));
    if write_png_frames {
        std::fs::create_dir_all(&frames_dir)?;
    }

    // Determine which graph to render: the master graph if present, else build a
    // presentation graph that composites the visible clip's own graph or a solid
    // fallback. For each frame we compile a per-frame runtime lazily (cached by
    // graph id) so we do not rebuild pipelines.
    let mut roots_cache: HashMap<String, RuntimeGraph> = HashMap::new();
    let mut router = Router::default();
    let media_worker = MediaWorker::new().ok();

    // Open a raw-RGBA pipe into ffmpeg once, for video codecs.
    let mut encoder = if write_png_frames {
        None
    } else {
        Some(flux_media::StreamingEncoder::start(
            &opts.out_dir.join(format!(
                "{}.{}",
                sanitize(&opts.project_name),
                match opts.codec {
                    EncodeCodec::Gif => "gif",
                    EncodeCodec::ProRes => "mov",
                    _ => "mp4",
                }
            )),
            opts.width,
            opts.height,
            flux_media::FileFrameRate {
                num: fps.num,
                den: fps.den,
            },
            opts.codec,
        )?)
    };

    let mut rendered = 0u64;

    // Audio is decoded once, up front, for the whole range: the same interleaved
    // buffer is (a) written to the WAV below and (b) analysed into the feature
    // timeline the frames are rendered from. Analysing before the loop matters
    // for AT017 — the features a frame sees are chosen by that frame's *source
    // timestamp*, so the picture cannot depend on how fast the renderer happened
    // to be going.
    let samples = mix_audio(project, start, duration)?;
    let timeline = crate::audio_features::timeline_for_range(project, start, &samples)?;

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
            None,
            Some(&timeline),
        )?;
        if let Some(cb) = &frame_cb {
            cb(i + 1, total, &px, opts.width, opts.height);
        }
        if let Some(enc) = encoder.as_mut() {
            enc.write_frame(&px)?;
        } else {
            let path = frames_dir.join(format!("frame_{:06}.png", i + 1));
            let png = flux_media::encode_png_rgba(opts.width, opts.height, &px)?;
            std::fs::write(&path, png)?;
        }
        rendered += 1;
        if let Some(cb) = &progress {
            cb(i + 1, total);
        }
    }

    // Audio: the deterministic stereo mix computed above.
    let wav_path = opts
        .out_dir
        .join(format!("{}.wav", sanitize(&opts.project_name)));
    flux_media::write_wav_stereo(
        &wav_path,
        project.settings.audio.sample_rate,
        &samples.samples,
    )?;

    // Finish the video stream (video codecs) or convert the PNG sequence.
    let mut video = None;
    if let Some(enc) = encoder {
        // Mux the audio track in a second pass so the WAV exists first.
        video = Some(enc.finish(Some(&wav_path))?);
    } else if matches!(opts.codec, EncodeCodec::JpegSequence) {
        if let Ok(worker) = MediaWorker::new() {
            let fps_str = format!("{}/{}", fps.num, fps.den);
            let _ = worker.encode_video_from_frames(
                &frames_dir,
                &fps_str,
                None,
                &opts.out_dir.join("unused"),
                EncodeCodec::JpegSequence,
            );
        }
    }

    Ok(ExportResult {
        frames: rendered,
        samples: samples.samples.len(),
        frames_dir,
        wav: Some(wav_path),
        video,
        feature_source: timeline.feature_source(),
        audio_windows: timeline.windows(),
    })
}

/// A reusable preview renderer: holds the GPU device, shader compiler, router and
/// graph cache across frames so the live editor does NOT recreate a GPU device
/// (and recompile shaders) on every preview. Rendering one frame is then cheap
/// (~a few ms) instead of hundreds.
pub struct PreviewRenderer {
    gpu: Gpu,
    catalog: Catalog,
    compiler: EffectCompiler,
    router: Router,
    roots_cache: HashMap<String, RuntimeGraph>,
    /// Analysed media audio for the current audio state, keyed by a signature of
    /// it. Scrubbing changes only the time, so the cache hits and no frame pays
    /// for a decode; an audio edit changes the signature and rebuilds once.
    audio_cache: Option<(String, Arc<crate::audio_features::MediaAudioTimeline>)>,
}

impl PreviewRenderer {
    pub fn new() -> FluxResult<Self> {
        let gpu = Gpu::new(BackendPref::Auto)?;
        let catalog = build_catalog();
        let compiler = EffectCompiler::new(&gpu);
        Ok(Self {
            gpu,
            catalog,
            compiler,
            router: Router::default(),
            roots_cache: HashMap::new(),
            audio_cache: None,
        })
    }

    /// The feature timeline for `project`'s audio over `range`, reused across
    /// frames until the audio state changes.
    fn audio_timeline(
        &mut self,
        project: &Project,
        start: RationalTime,
        duration: RationalTime,
    ) -> Arc<crate::audio_features::MediaAudioTimeline> {
        use crate::audio_features::MediaAudioTimeline;
        let signature = audio_signature(project, start, duration);
        let cached = match self.audio_cache.as_ref() {
            Some((key, timeline)) if *key == signature => Some(timeline.clone()),
            _ => None,
        };
        if let Some(timeline) = cached {
            return timeline;
        }
        let timeline = match mix_audio(project, start, duration) {
            Ok(mix) => match crate::audio_features::timeline_for_range(project, start, &mix) {
                Ok(t) => Arc::new(t),
                Err(_) => Arc::new(MediaAudioTimeline::empty(
                    start,
                    project.settings.audio.sample_rate,
                )),
            },
            Err(_) => Arc::new(MediaAudioTimeline::empty(
                start,
                project.settings.audio.sample_rate,
            )),
        };
        self.audio_cache = Some((signature, timeline.clone()));
        timeline
    }

    /// Render a single frame of `project` at `time` to packed RGBA8.
    ///
    /// If `time` is past the end of the timeline content, it is clamped to the
    /// last sampleable instant so the preview holds the final frame instead of
    /// rendering blank (which showed as white in the editor).
    pub fn render(
        &mut self,
        project: &Project,
        time: RationalTime,
        width: u32,
        height: u32,
    ) -> FluxResult<Vec<u8>> {
        let range = flux_engine::timeline::export_range(project);
        let start = range.start;
        let end = start.add(range.duration).unwrap_or(start);
        let mut t = time;
        if t < start {
            t = start;
        } else if t >= end {
            let one_us = RationalTime::new(1, 1_000_000).unwrap_or(start);
            t = end.sub(one_us).unwrap_or(start);
            if t < start {
                t = start;
            }
        }
        let audio = self.audio_timeline(project, start, range.duration);
        let frame_ctx = FrameContext::for_project(project, t, 0);
        render_single_frame(
            &self.gpu,
            &self.catalog,
            &mut self.compiler,
            project,
            &frame_ctx,
            &mut self.roots_cache,
            &mut self.router,
            None,
            width,
            height,
            None,
            Some(&audio),
        )
    }

    /// Render an explicit graph (the node editor's canvas) — bypasses the
    /// timeline entirely. `time` still drives animated/uniform parameters.
    pub fn render_graph(
        &mut self,
        graph: &flux_domain::Graph,
        project: &Project,
        time: RationalTime,
        width: u32,
        height: u32,
    ) -> FluxResult<Vec<u8>> {
        let range = flux_engine::timeline::export_range(project);
        let audio = self.audio_timeline(project, range.start, range.duration);
        let frame_ctx = FrameContext::for_project(project, time, 0);
        let media_worker = MediaWorker::new().ok();
        render_single_frame(
            &self.gpu,
            &self.catalog,
            &mut self.compiler,
            project,
            &frame_ctx,
            &mut self.roots_cache,
            &mut self.router,
            media_worker.as_ref(),
            width,
            height,
            Some(graph),
            Some(&audio),
        )
    }
}

/// Cheap, ordered fingerprint of everything in `project` that changes the
/// analysed audio over a range. Deliberately *not* a hash of the whole document:
/// colour or graph edits must not force a re-decode.
fn audio_signature(project: &Project, start: RationalTime, duration: RationalTime) -> String {
    let mut s = format!(
        "sr={};n={};r={}/{};d={}/{}",
        project.settings.audio.sample_rate,
        project.settings.audio.channels,
        start.numerator(),
        start.denominator(),
        duration.numerator(),
        duration.denominator()
    );
    for track in project.timeline.ordered_tracks() {
        if track.kind != flux_domain::TrackKind::Audio {
            continue;
        }
        s.push_str(&format!(
            "|T{}:{}:{}:{}",
            track.id, track.muted, track.gain_db, track.solo
        ));
        for clip in project.timeline.track_clips(&track.id) {
            let asset = match &clip.source {
                flux_domain::ClipSource::Asset { asset_id } => project
                    .assets
                    .get(asset_id)
                    .map(|a| format!("{}#{}", a.original_path, a.fingerprint))
                    .unwrap_or_else(|| format!("missing:{asset_id}")),
                other => format!("{other:?}"),
            };
            s.push_str(&format!(
                "|C{}:{}/{}-{}{}:{}/{}:{}:{}:{}",
                clip.id,
                clip.range.start.numerator(),
                clip.range.start.denominator(),
                clip.range.duration.numerator(),
                clip.range.duration.denominator(),
                clip.source_range.start.numerator(),
                clip.source_range.duration.numerator(),
                clip.audio.gain_db,
                clip.audio.pan,
                asset,
            ));
        }
    }
    s
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
    graph_override: Option<&flux_domain::Graph>,
    audio: Option<&crate::audio_features::MediaAudioTimeline>,
) -> FluxResult<Vec<u8>> {
    // The render targets are at the export resolution, so the frame context's
    // dimensions must match them (otherwise full-frame source sampling is wrong).
    let mut ctx = ctx_in.clone();
    ctx.output_width = width;
    ctx.output_height = height;
    ctx.source_width = width;
    ctx.source_height = height;
    // Attach the audio evidence for this frame's exact project time. Until now
    // this path always rendered with `AudioSnapshot::default()`, so the `audio`
    // uniform every generated shader binds was zeros. When there is no audio
    // timeline — a generator-only project, or a caller that has not decoded any
    // — the silent document-only baseline is kept and *named*, not faked.
    let snapshot = match audio.and_then(|t| t.evidence_at(ctx.project_time)) {
        Some(ev) => {
            ctx.audio = ev.audio;
            ctx.music = ev.music;
            ctx.feature_source = ev.source;
            ev.features
        }
        None => flux_domain::params::FeatureSnapshot::default(),
    };
    let ctx = &ctx;
    // Pick the TOP-MOST visible clip at this time (last in bottom-to-top order),
    // matching normal compositor semantics: a newly added scene on a higher track
    // is what you see, and it changes when you click a different generator/effect.
    // (Full multi-track blending is a larger feature; this renders the active clip
    // faithfully so edits are visible immediately.)
    let items = render_items(project, ctx.project_time);
    // The node editor's composition root (master graph) wins over the timeline's
    // top-most clip, so preview and export render exactly what the canvas shows.
    // When no master graph is set, behaviour is the legacy top-most-clip path.
    let chosen_graph_id = if graph_override.is_some() {
        None
    } else if let Some(mid) = project.timeline.master_graph_id.as_ref() {
        Some(mid.clone())
    } else {
        items.iter().rev().find_map(|it| match it {
            RenderItem::Clip { clip, .. } => clip_graph_for(project, clip),
            _ => None,
        })
    };

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
    let graph = match graph_override {
        Some(g) => g.clone(),
        None => match graph {
            Some(g) => g,
            None => solid_fallback(),
        },
    };

    // Cache key = graph CONTENT hash (not id), so edits rebuild the pipeline and
    // a stale cached runtime can never render a blank/old frame. The render size
    // is part of the key: the runtime allocates one texture per node at a fixed
    // resolution, so reusing a 320x180 preview runtime for a 1280x720 export
    // overran the source texture and aborted the process.
    let key = format!("{}@{}x{}", graph_content_key(&graph), width, height);
    if !roots_cache.contains_key(&key) {
        // Bound the cache: keep it small so it cannot grow unbounded over an
        // editing session.
        if roots_cache.len() >= 12 {
            roots_cache.clear();
        }
        let rt = RuntimeGraph::activate(gpu, compiler, catalog, &graph, width, height, 1)?;
        roots_cache.insert(key.clone(), rt);
    }
    let rt = roots_cache.get_mut(&key).unwrap();

    // Resolve parameters for every node in the graph.
    let mut resolved: HashMap<String, NodeParamsBuffer> = HashMap::new();
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
            let node = graph.node(&pass.node_id);
            for slot in &layout.slots {
                // Apply the node's stored param (colour / vec / enum / bool / int)
                // — without this, only floats changed and everything else stayed at
                // the manifest default (the "clicking does nothing" bug).
                if let Some(pv) = node.and_then(|n| n.params.get(&slot.id)) {
                    apply_param_value(&mut buf, slot, pv, &e.manifest);
                }
                // Resolved (keyframed/modulated) value wins for scalar slots.
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

/// Write a node's `ParamValue` into the uniform buffer slot according to the
/// slot's kind. Handles colour/vec2/vec3/vec4, int/bool/enum (enum string ->
/// index using the manifest options). Ignores mismatched value kinds.
fn apply_param_value(
    buf: &mut NodeParamsBuffer,
    slot: &flux_catalog::pack::ParamSlot,
    pv: &flux_domain::ParamValue,
    manifest: &flux_catalog::Manifest,
) {
    use flux_catalog::pack::SlotKind;
    use flux_domain::ParamValue;
    match slot.kind {
        SlotKind::F32 => {
            let v = match pv {
                ParamValue::Float { value } => *value,
                ParamValue::Int { value } => *value as f64,
                ParamValue::Bool { value } => {
                    if *value {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => return,
            };
            buf.floats[slot.vec4_index][slot.component] = v as f32;
        }
        SlotKind::Vec2 => {
            if let ParamValue::Vec2 { value } = pv {
                let f = &mut buf.floats[slot.vec4_index];
                f[slot.component] = value[0] as f32;
                f[slot.component + 1] = value[1] as f32;
            }
        }
        SlotKind::Vec3 => {
            if let ParamValue::Vec3 { value } = pv {
                let f = &mut buf.floats[slot.vec4_index];
                f[slot.component] = value[0] as f32;
                f[slot.component + 1] = value[1] as f32;
                f[slot.component + 2] = value[2] as f32;
            }
        }
        SlotKind::Vec4 => {
            if let ParamValue::Color { value } = pv {
                let f = &mut buf.floats[slot.vec4_index];
                for i in 0..4 {
                    f[i] = value[i] as f32;
                }
            }
        }
        SlotKind::U32 => {
            let v = match pv {
                ParamValue::Int { value } => (*value).max(0) as u32,
                ParamValue::Bool { value } => *value as u32,
                ParamValue::Float { value } => (*value).max(0.0) as u32,
                ParamValue::Enum { value } => manifest
                    .parameters
                    .iter()
                    .find(|p| p.id == slot.id)
                    .and_then(|p| p.options.iter().position(|o| o == value))
                    .unwrap_or(0) as u32,
                _ => return,
            };
            buf.uints[slot.vec4_index][slot.component] = v;
        }
    }
}

/// Stable content hash of a graph (id + nodes + edges + output + params), used as
/// the runtime cache key so edits always invalidate the cached pipeline.
fn graph_content_key(graph: &flux_domain::Graph) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    graph.id.hash(&mut h);
    graph.output_node.hash(&mut h);
    for n in &graph.nodes {
        n.id.hash(&mut h);
        n.type_id.hash(&mut h);
        n.type_version.hash(&mut h);
        n.bypass.hash(&mut h);
        for (k, v) in &n.params {
            k.hash(&mut h);
            format!("{v:?}").hash(&mut h);
        }
    }
    for e in &graph.edges {
        e.from_node.hash(&mut h);
        e.from_port.hash(&mut h);
        e.to_node.hash(&mut h);
        e.to_port.hash(&mut h);
    }
    format!("{:016x}", h.finish())
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
pub fn mix_audio(
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
    let mut decoded_clips = 0usize;

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
            decoded_clips += 1;
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
    Ok(MixedAudio {
        samples: out,
        decoded_clips,
    })
}

pub struct MixedAudio {
    pub samples: Vec<f32>,
    /// How many audio clips actually decoded. This is what distinguishes "the
    /// project has audio and it is silent" from "there was nothing to analyse" —
    /// the feature timeline must not claim `valid = true` measurements when the
    /// decode produced nothing.
    pub decoded_clips: usize,
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
    // A contact sheet is effects composited over `gen.plasma` with no audio
    // track at all. This is the explicit document-only baseline, taken through
    // the same type the export path uses, so the two can never drift apart and
    // so the row records *why* its features are the silent ones.
    let sheet_baseline = crate::audio_features::MediaAudioTimeline::empty(
        RationalTime::ZERO,
        project.settings.audio.sample_rate,
    );

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
        let snap = sheet_baseline
            .evidence_at(ctx.project_time)
            .map(|e| e.features)
            .unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_names_map_to_encoders() {
        assert_eq!(codec_from_name("h264"), EncodeCodec::H264);
        assert_eq!(codec_from_name("MP4"), EncodeCodec::H264);
        assert_eq!(codec_from_name("hevc"), EncodeCodec::Hevc);
        assert_eq!(codec_from_name("prores"), EncodeCodec::ProRes);
        assert_eq!(codec_from_name("gif"), EncodeCodec::Gif);
        assert_eq!(codec_from_name("png"), EncodeCodec::None);
        assert_eq!(codec_from_name("unknown"), EncodeCodec::H264);
    }

    #[test]
    fn document_only_run_reports_document_only() {
        // A deterministic export (no live audio, no media audio timeline) must
        // report `FeatureSource::DocumentOnly` and zero audio windows. This is
        // the audit trail for the determinism firewall: if an export ever
        // reports `Live`, the path is broken.
        let project = flux_domain::Project::new("audit", 1);
        let dir = std::env::temp_dir().join(format!("flux-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let result = export_project(
            &project,
            &ExportOptions {
                out_dir: dir.clone(),
                project_name: "audit".into(),
                codec: flux_media::EncodeCodec::None,
                width: 32,
                height: 32,
                range: None,
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            None,
        )
        .unwrap();
        assert_eq!(
            result.feature_source,
            flux_domain::params::FeatureSource::DocumentOnly,
            "a project with no audio timeline must report DocumentOnly"
        );
        assert_eq!(
            result.audio_windows, 0,
            "a project with no audio timeline must report zero windows"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
