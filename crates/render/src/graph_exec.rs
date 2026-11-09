//! Graph execution on the GPU: walking the compiled pass list, binding inputs,
//! uploading uniforms, and rendering into ping-ponged targets.
//!
//! The renderer never compiles pipelines synchronously inside presentation; the
//! executor compiles all reachable pipelines once on graph activation and keeps
//! the last valid graph if a new one fails.

use crate::gpu::Gpu;
use crate::pipeline::{render_target, AudioFeaturesGpu, EffectCompiler, FrameUniformGpu, NodeParamsBuffer};
use flux_catalog::{Catalog, CatalogEntry};
use flux_domain::{ErrorCode, FluxError, FluxResult, Graph};
use flux_engine::frame::{AudioSnapshot, FrameContext};
use flux_engine::graph::{compile, CompiledGraph, InputBinding, Registry};
use std::collections::HashMap;

/// A runtime graph ready to execute frames.
pub struct RuntimeGraph {
    pub graph_id: String,
    pub compiled: CompiledGraph,
    pub graph_version: u64,
    /// Two ping-pong scratch targets.
    targets: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// Dedicated final output target (never used as a pass input).
    output_target: (wgpu::Texture, wgpu::TextureView),
    node_uniforms: HashMap<String, NodeParamsBuffer>,
    frame_buf: wgpu::Buffer,
    audio_buf: wgpu::Buffer,
    params_bufs: HashMap<String, wgpu::Buffer>,
    bind_groups: HashMap<String, wgpu::BindGroup>,
    input_bind_groups: HashMap<String, wgpu::BindGroup>,
    pub preview_scale: f32,
}

impl RuntimeGraph {
    /// Build and compile pipelines for a graph. Returns an error if the graph is
    /// invalid or a shader fails to compile; the caller keeps the last valid
    /// runtime in that case.
    pub fn activate(
        gpu: &Gpu,
        compiler: &mut EffectCompiler,
        catalog: &Catalog,
        graph: &Graph,
        width: u32,
        height: u32,
        graph_version: u64,
    ) -> FluxResult<Self> {
        let reg = Registry::new(catalog);
        let compiled = compile(graph, &reg, graph_version)?;
        // Compile every referenced pipeline up front.
        for pass in &compiled.passes {
            let Some(entry) = catalog.get(&pass.type_id) else {
                return Err(FluxError::new(
                    ErrorCode::GraphTypeMismatch,
                    "render",
                    format!("missing type {}", pass.type_id),
                ));
            };
            compiler
                .ensure_pipeline(gpu, entry)
                .map_err(|e| FluxError::new(ErrorCode::ShaderCompile, "render", e.message))?;
        }

        // Two ping-pong targets keep read/write from aliasing within a pass.
        let targets = vec![render_target(gpu, width, height), render_target(gpu, width, height)];
        let output_target = render_target(gpu, width, height);
        let frame_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-uniform"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let audio_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("audio-uniform"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut node_uniforms = HashMap::new();
        let mut params_bufs = HashMap::new();
        let mut bind_groups = HashMap::new();
        let mut input_bind_groups = HashMap::new();
        for pass in &compiled.passes {
            let Some(entry) = catalog.get(&pass.type_id) else { continue };
            let (_, layout) = flux_catalog::pack::params_wgsl(&entry.manifest.parameters);
            let buf = NodeParamsBuffer {
                floats: vec![[0.0; 4]; layout.f_count],
                uints: vec![[0u32; 4]; layout.u_count],
            };
            let bytes = buf.bytes();
            let ubo = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("node-params"),
                size: bytes.len().max(256) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue.write_buffer(&ubo, 0, &bytes);
            // Bind group 0: frame, params, audio.
            let bgl0 = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bind0"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(96),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(48),
                        },
                        count: None,
                    },
                ],
            });
            let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bind0-group"),
                layout: &bgl0,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: ubo.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: audio_buf.as_entire_binding() },
                ],
            });
            // Bind group 1: source/mask/sampler + extras (using dummy views).
            let bgl1 = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bind1"),
                entries: &[
                    tex_entry(0),
                    tex_entry(1),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            let bg1 = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bind1-group"),
                layout: &bgl1,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&targets[0].1) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&compiler.white_mask) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(compiler.sampler()) },
                ],
            });
            node_uniforms.insert(pass.node_id.clone(), buf);
            params_bufs.insert(pass.node_id.clone(), ubo);
            bind_groups.insert(pass.node_id.clone(), bg);
            input_bind_groups.insert(pass.node_id.clone(), bg1);
            let _ = entry;
        }

        Ok(Self {
            graph_id: graph.id.clone(),
            compiled,
            graph_version,
            targets,
            output_target,
            node_uniforms,
            frame_buf,
            audio_buf,
            params_bufs,
            bind_groups,
            input_bind_groups,
            preview_scale: 1.0,
        })
    }

    pub fn target_view(&self) -> &wgpu::TextureView {
        &self.output_target.1
    }

    pub fn target_texture(&self) -> wgpu::Texture {
        self.output_target.0.clone()
    }

    /// Execute a frame. `source` is the incoming scope texture (for the graph's
    /// Source node). All node uniforms are supplied by the caller, already
    /// resolved. The final texture is left in the last target.
    pub fn execute(
        &mut self,
        gpu: &Gpu,
        compiler: &EffectCompiler,
        catalog: &Catalog,
        ctx: &FrameContext,
        source: &wgpu::TextureView,
        resolved: &HashMap<String, NodeParamsBuffer>,
        wet: f32,
    ) -> FluxResult<()> {
        // Upload frame + audio uniforms.
        let uniform = FrameUniformGpu {
            dims: ctx.uniform(wet).dims,
            sampling: ctx.uniform(wet).sampling,
            clock: ctx.uniform(wet).clock,
            counters: ctx.uniform(wet).counters,
            music: ctx.uniform(wet).music,
            controls: ctx.uniform(wet).controls,
        };
        gpu.queue.write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&uniform));
        let a: AudioSnapshot = ctx.audio;
        let audio = AudioFeaturesGpu { bands: a.bands, extra: a.extra, params: a.params };
        gpu.queue.write_buffer(&self.audio_buf, 0, bytemuck::bytes_of(&audio));

        // Upload each node's resolved parameters.
        for pass in &self.compiled.passes {
            if let (Some(buf), Some(ubo)) = (resolved.get(&pass.node_id), self.params_bufs.get(&pass.node_id)) {
                let bytes = buf.bytes();
                gpu.queue.write_buffer(ubo, 0, &bytes);
            }
        }

        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("graph") });

        // Intermediate passes alternate between the two scratch targets; the
        // final pass writes the dedicated output target. A pass must never read
        // and write the same texture in one render pass, so the write slot is
        // chosen to differ from the read slot.
        let mut last_written: Option<usize> = None; // 0 or 1 scratch, or 2 = output
        let pass_count = self.compiled.passes.len();

        for (pass_idx, pass) in self.compiled.passes.iter().enumerate() {
            let Some(entry) = catalog.get(&pass.type_id) else { continue };
            let Some(pipeline) = compiler.pipeline(&pass.type_id) else { continue };
            let is_last = pass_idx + 1 == pass_count;

            // Resolve the input texture view (scratch/output) or the external source.
            let read_scratch: Option<usize> = last_written;
            let input_view: &wgpu::TextureView = match pass.inputs.get("source") {
                Some(InputBinding::NodeOutput(_)) | Some(InputBinding::ScopeSource) => match read_scratch {
                    Some(2) => &self.output_target.1,
                    Some(i) => &self.targets[i].1,
                    None => source,
                },
                _ => source,
            };

            let has_source = entry.manifest.inputs.iter().any(|p| p.id == "source");
            // Scope-source nodes present the external texture even though they
            // declare no typed input port.
            let is_scope_source = pass.type_id == "flux.source" || pass.type_id == "flux.mask_input";
            let src_binding: &wgpu::TextureView = if has_source || is_scope_source {
                // For a scope source, the passed-in `source` view is the frame.
                if is_scope_source {
                    source
                } else {
                    input_view
                }
            } else {
                &compiler.dummy_tex
            };
            let input_layout = make_input_layout(gpu);
            let input_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("per-pass-input"),
                layout: &input_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src_binding) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&compiler.white_mask) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(compiler.sampler()) },
                ],
            });

            // Choose write slot: never equal to the read scratch slot.
            let write_slot: usize = if is_last {
                2
            } else if read_scratch == Some(0) {
                1
            } else {
                0
            };
            let write_view: &wgpu::TextureView = if write_slot == 2 {
                &self.output_target.1
            } else {
                &self.targets[write_slot].1
            };

            let bgl0 = make_bind0_layout(gpu);
            let bg0 = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bind0-per-pass"),
                layout: &bgl0,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.frame_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: self.params_bufs[&pass.node_id].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: self.audio_buf.as_entire_binding() },
                ],
            });

            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("effect-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: write_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                rpass.set_pipeline(&pipeline.pipeline);
                rpass.set_bind_group(0, &bg0, &[]);
                rpass.set_bind_group(1, &input_bg, &[]);
                rpass.draw(0..3, 0..1);
            }
            last_written = Some(write_slot);
        }

        // If the last pass did not write the output target, copy its result in.
        if let Some(slot) = last_written {
            if slot != 2 {
                let src_tex = &self.targets[slot].0;
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: src_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.output_target.0,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.output_target.0.width(),
                        height: self.output_target.0.height(),
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        gpu.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

fn make_input_layout(gpu: &Gpu) -> wgpu::BindGroupLayout {
    gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("input-layout"),
        entries: &[
            tex_entry(0),
            tex_entry(1),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn make_bind0_layout(gpu: &Gpu) -> wgpu::BindGroupLayout {
    gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bind0-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(96),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(48),
                },
                count: None,
            },
        ],
    })
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Compile a single manifest's pipeline and dispose immediately. Returns whether
/// the device accepted it (used by the shader-validation test).
pub fn validate_shader(gpu: &Gpu, entry: &CatalogEntry) -> FluxResult<()> {
    let mut compiler = EffectCompiler::new(gpu);
    compiler.ensure_pipeline(gpu, entry)
}
