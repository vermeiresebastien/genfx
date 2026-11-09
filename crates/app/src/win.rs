//! Native window + egui integration for the Flux editor.
//!
//! Creates a winit window with an egui-wgpu renderer and runs the editor each
//! frame. The editor renders the selected generator through the same GPU graph
//! used by export and displays the result as a preview.

use crate::editor::UiState;
use crate::samples;
use flux_domain::{Clip, ClipSource, ErrorCode, FluxError, FluxResult, Project, RationalTime, Size, TimeRange};
use flux_render::gpu::{BackendPref, Gpu};
use flux_render::graph_exec::RuntimeGraph;
use flux_render::pipeline::{EffectCompiler, NodeParamsBuffer};
use flux_render::preview::read_preview;
use std::collections::HashMap;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::Window;

pub fn run() -> FluxResult<()> {
    let event_loop = EventLoop::new().map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "ui", e.to_string()))?;
    let mut app = App::new();
    event_loop
        .run_app(&mut app)
        .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "ui", e.to_string()))?;
    Ok(())
}

struct App {
    state: Option<State>,
    project: Project,
    ui: UiState,
}

struct State {
    window: Arc<Window>,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    graph_cache: HashMap<String, RuntimeGraph>,
    compiler: Option<EffectCompiler>,
    preview_tex: Option<egui::TextureHandle>,
    frame_index: u64,
}

impl App {
    fn new() -> Self {
        let project = samples::demo_generator_project("gen.plasma", &["fx.bloom"], 8);
        Self {
            state: None,
            project,
            ui: UiState {
                selected_clip: None,
                selected_node: None,
                library_search: String::new(),
                playing: true,
            },
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Flux")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0f64));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let gpu = Gpu::new(BackendPref::Auto).expect("gpu");

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).expect("surface");
        let size = window.inner_size();
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &config);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(&gpu.device, format, None, 1, false);

        self.state = Some(State {
            window,
            egui_ctx,
            egui_state,
            renderer,
            gpu,
            surface,
            config,
            graph_cache: HashMap::new(),
            compiler: None,
            preview_tex: None,
            frame_index: 0,
        });
        // ensure compiler initialized
        if let Some(s) = self.state.as_mut() {
            s.compiler = Some(EffectCompiler::new(&s.gpu));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else { return };
        let response = state.egui_state.on_window_event(&state.window, &event);
        if response.consumed {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    state.config.width = size.width;
                    state.config.height = size.height;
                    state.surface.configure(&state.gpu.device, &state.config);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.frame() {
                    eprintln!("frame error: {e:?}");
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(s) = &self.state {
            s.window.request_redraw();
        }
    }
}

impl App {
    fn frame(&mut self) -> FluxResult<()> {
        let state = self.state.as_mut().unwrap();
        let raw_input = state.egui_state.take_egui_input(&state.window);

        // Render preview via the engine before building UI.
        let preview = self.render_preview(state)?;
        if let Some((w, h, px)) = preview {
            let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
            match &mut state.preview_tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => {
                    state.preview_tex = Some(state.egui_ctx.load_texture("preview", img, egui::TextureOptions::LINEAR));
                }
            }
        }

        let preview_id = state.preview_tex.clone();
        let full_output = state.egui_ctx.run(raw_input, |ctx| {
            self.ui(ctx, &preview_id);
        });
        state.egui_state.handle_platform_output(&state.window, full_output.platform_output);
        let paint_jobs = state.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        for (id, delta) in &full_output.textures_delta.set {
            state.renderer.update_texture(&state.gpu.device, &state.gpu.queue, *id, delta);
        }
        let screen = full_output.screen_descriptor;

        let frame = match state.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                state.surface.configure(&state.gpu.device, &state.config);
                return Ok(());
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = state.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("egui") });
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [state.config.width, state.config.height],
            pixels_per_point: state.egui_ctx.pixels_per_point(),
        };
        state.renderer.update_buffers(&state.gpu.device, &state.gpu.queue, &mut encoder, &paint_jobs, &screen);
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.08, g: 0.08, b: 0.09, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            state.renderer.render(&mut rpass.forget_lifetime(), &paint_jobs, &screen);
        }
        state.gpu.queue.submit(Some(encoder.finish()));
        frame.present();
        for id in &full_output.textures_delta.free {
            state.renderer.free_texture(id);
        }
        if self.ui.playing {
            state.frame_index = state.frame_index.wrapping_add(1);
        }
        Ok(())
    }

    fn render_preview(&mut self, state: &mut State) -> FluxResult<Option<(u32, u32, Vec<u8>)>> {
        let size = self.project.settings.size;
        let t = {
            let fps = self.project.settings.frame_rate;
            let frame = state.frame_index;
            let num = frame as i64 * fps.den as i64;
            RationalTime::new(num, fps.num).unwrap_or(RationalTime::ZERO)
        };
        let ctx = flux_engine::frame::FrameContext::for_project(&self.project, t, state.frame_index);

        // The active graph is the first clip's graph (or a generator graph).
        let Some((graph_id, gen_id)) = self.active_graph() else {
            return Ok(None);
        };
        let graph = if let Some(g) = self.project.graphs.get(&graph_id) {
            g.clone()
        } else {
            samples::generator_graph(&gen_id)
        };
        let key = graph.id.clone();
        if !state.graph_cache.contains_key(&key) {
            let compiler = state.compiler.as_mut().unwrap();
            let rt = RuntimeGraph::activate(&state.gpu, compiler, &BUILD_CATALOG(), &graph, size.width, size.height, 1)?;
            state.graph_cache.insert(key.clone(), rt);
        }
        let catalog = BUILD_CATALOG();
        let rt = state.graph_cache.get_mut(&key).unwrap();
        let mut resolved: HashMap<String, NodeParamsBuffer> = HashMap::new();
        for pass in &rt.compiled.passes {
            if let Some(e) = catalog.get(&pass.type_id) {
                resolved.insert(pass.node_id.clone(), NodeParamsBuffer::defaults(&e.manifest));
            }
        }
        let src = state.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let src_view = src.create_view(&Default::default());
        let compiler = state.compiler.as_ref().unwrap();
        rt.execute(&state.gpu, compiler, &catalog, &ctx, &src_view, &resolved, 1.0)?;
        let out = read_preview(&state.gpu, &rt.target_texture(), size.width, size.height)?;
        Ok(Some(out))
    }

    fn active_graph(&self) -> Option<(String, String)> {
        for track in self.project.timeline.ordered_tracks() {
            for clip in self.project.timeline.track_clips(&track.id) {
                if let Some(gid) = &clip.graph_id {
                    return Some((gid.clone(), String::new()));
                }
                if let ClipSource::GeneratorScene { generator_id } = &clip.source {
                    return Some((format!("graph:{}", generator_id), generator_id.clone()));
                }
            }
        }
        None
    }

    fn ui(&mut self, ctx: &egui::Context, preview: &Option<egui::TextureHandle>) {
        egui::TopBottomPanel::top("topbar").exact_height(48.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading("Flux");
                ui.separator();
                ui.label(&self.project.name);
                ui.separator();
                if ui.button(if self.ui.playing { "Pause" } else { "Play" }).clicked() {
                    self.ui.playing = !self.ui.playing;
                }
                ui.label(format!("frame {}", self.state.as_ref().map(|s| s.frame_index).unwrap_or(0)));
            });
        });

        egui::SidePanel::left("library").default_width(240.0).show(ctx, |ui| {
            ui.heading("Library");
            ui.text_edit_singleline(&mut self.ui.library_search);
            let q = self.ui.library_search.to_lowercase();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let catalog = BUILD_CATALOG();
                for e in &catalog.entries {
                    if e.manifest.id.starts_with("gen.") || e.manifest.id.starts_with("fx.") {
                        if q.is_empty() || e.manifest.name.to_lowercase().contains(&q) || e.manifest.id.contains(&q) {
                            if ui.button(format!("{}  ({})", e.manifest.name, e.manifest.category)).clicked() {
                                self.set_generator(&e.manifest.id);
                            }
                        }
                    }
                }
            });
        });

        egui::SidePanel::right("inspector").default_width(300.0).show(ctx, |ui| {
            ui.heading("Inspector");
            ui.separator();
            ui.label(format!("Size: {}x{}", self.project.settings.size.width, self.project.settings.size.height));
            ui.label(format!("Rate: {}/{}", self.project.settings.frame_rate.num, self.project.settings.frame_rate.den));
            ui.label(format!("Duration: {:.2}s", self.project.timeline.range.duration.as_f64()));
            ui.separator();
            if ui.button("Export (PNG+WAV+MP4)").clicked() {
                let dir = PathBuf::from("out");
                match crate::export::export_default(&self.project, &dir) {
                    Ok(r) => {
                        self.status = format!("Exported {} frames to {}", r.frames, r.dir());
                    }
                    Err(e) => self.status = format!("Export failed: {e:?}"),
                }
            }
            if ui.button("Save project").clicked() {
                let path = PathBuf::from("project.flux.json");
                match crate::save_project(&self.project, &path) {
                    Ok(_) => self.status = format!("Saved {}", path.display()),
                    Err(e) => self.status = format!("Save failed: {e:?}"),
                }
            }
            ui.separator();
            ui.label("Scenes");
            let names = ["gen.plasma", "gen.clouds", "gen.mandelbrot", "gen.nebula", "gen.aurora", "gen.fluid"];
            for n in names {
                if ui.button(n).clicked() {
                    self.set_generator(n);
                }
            }
            ui.separator();
            ui.monospace(&self.status);
        });

        egui::TopBottomPanel::bottom("timeline").exact_height(180.0).show(ctx, |ui| {
            ui.heading("Timeline");
            ui.label(format!("Tracks: {}", self.project.timeline.ordered_tracks().len()));
            for track in self.project.timeline.ordered_tracks() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} [{}]", track.name, track_clip_count(track)));
                });
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = preview {
                let avail = ui.available_size();
                let aspect = self.project.settings.size.width as f32 / self.project.settings.size.height as f32;
                let mut w = avail.x - 8.0;
                let mut h = w / aspect;
                if h > avail.y - 8.0 {
                    h = avail.y - 8.0;
                    w = h * aspect;
                }
                ui.add(egui::Image::new((tex.id(), egui::vec2(w.max(1.0), h.max(1.0)))));
            } else {
                ui.label("No preview available");
            }
        });
    }

    fn set_generator(&mut self, generator_id: &str) {
        self.project = samples::demo_generator_project(generator_id, &["fx.bloom"], 8);
        if let Some(s) = self.state.as_mut() {
            s.graph_cache.clear();
        }
        self.status = format!("Scene: {generator_id}");
    }
}

fn track_clip_count(track: &Track) -> usize {
    track.clip_order.len()
}

thread_local! {
    static CATALOG: flux_catalog::Catalog = flux_catalog::build_catalog();
}

fn BUILD_CATALOG() -> &'static flux_catalog::Catalog {
    // Leak once: the catalog is immutable for the process lifetime.
    use std::sync::OnceLock;
    static ONCE: OnceLock<flux_catalog::Catalog> = OnceLock::new();
    ONCE.get_or_init(flux_catalog::build_catalog)
}
