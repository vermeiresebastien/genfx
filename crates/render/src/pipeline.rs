//! Effect pipeline: compiles a catalog manifest's WGSL into a render pipeline,
//! manages node uniforms, and executes the pass. One pipeline is shared per
//! `type_id`; per-node state is a uniform buffer plus bind group.

use crate::gpu::Gpu;
use flux_catalog::{shader_gen, Manifest};
use flux_domain::{ErrorCode, FluxError, FluxResult};
use std::collections::HashMap;

/// Frame uniform mirror with bytemuck Pod/Zeroable. Layout matches WGSL exactly
/// (96 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameUniformGpu {
    pub dims: [u32; 4],
    pub sampling: [f32; 4],
    pub clock: [f32; 4],
    pub counters: [u32; 4],
    pub music: [f32; 4],
    pub controls: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AudioFeaturesGpu {
    pub bands: [f32; 4],
    pub extra: [f32; 4],
    pub params: [f32; 4],
}

/// The uniform buffer holding NodeParams. Allocation is padded to 256 bytes so
/// the same layout works for every manifest.
#[derive(Debug, Clone)]
pub struct NodeParamsBuffer {
    pub floats: Vec<[f32; 4]>,
    pub uints: Vec<[u32; 4]>,
}

impl NodeParamsBuffer {
    /// Build a parameter buffer from a manifest's declared defaults.
    pub fn defaults(manifest: &flux_catalog::Manifest) -> Self {
        let (_, layout) = flux_catalog::pack::params_wgsl(&manifest.parameters);
        let mut floats = vec![[0.0f32; 4]; layout.f_count];
        let mut uints = vec![[0u32; 4]; layout.u_count];
        for slot in &layout.slots {
            let Some(p) = manifest.parameters.iter().find(|p| p.id == slot.id) else {
                continue;
            };
            match (slot.kind, &p.default) {
                (flux_catalog::pack::SlotKind::F32, v) => {
                    if let Some(x) = v.as_f64() {
                        floats[slot.vec4_index][slot.component] = x as f32;
                    }
                }
                (flux_catalog::pack::SlotKind::Vec2, v) => {
                    if let Some(a) = v.as_array() {
                        for (i, x) in a.iter().enumerate() {
                            floats[slot.vec4_index][slot.component + i] =
                                x.as_f64().unwrap_or(0.0) as f32;
                        }
                    }
                }
                (flux_catalog::pack::SlotKind::Vec3, v) => {
                    if let Some(a) = v.as_array() {
                        for (i, x) in a.iter().enumerate() {
                            floats[slot.vec4_index][slot.component + i] =
                                x.as_f64().unwrap_or(0.0) as f32;
                        }
                    }
                }
                (flux_catalog::pack::SlotKind::Vec4, v) => {
                    if let Some(a) = v.as_array() {
                        for (i, x) in a.iter().enumerate().take(4) {
                            floats[slot.vec4_index][i] = x.as_f64().unwrap_or(0.0) as f32;
                        }
                    }
                }
                (flux_catalog::pack::SlotKind::U32, v) => {
                    let val = if let Some(x) = v.as_i64() {
                        x as u32
                    } else if let Some(b) = v.as_bool() {
                        b as u32
                    } else if let Some(s) = v.as_str() {
                        // enum: index is resolved by the caller; default to 0
                        let _ = s;
                        0
                    } else {
                        0
                    };
                    uints[slot.vec4_index][slot.component] = val;
                }
            }
        }
        Self { floats, uints }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.floats.len() + self.uints.len()) * 16);
        for f in &self.floats {
            out.extend_from_slice(bytemuck::cast_slice(f));
        }
        for u in &self.uints {
            out.extend_from_slice(bytemuck::cast_slice(u));
        }
        // pad to a multiple of 16 (arrays already are)
        while out.len() % 16 != 0 {
            out.push(0);
        }
        out
    }
}

/// Compiled shader module for a single node type.
pub struct EffectPipeline {
    pub type_id: String,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    /// Uniform buffer size in bytes.
    pub uniform_size: u64,
    pub has_extra_inputs: bool,
}

pub struct EffectCompiler {
    modules: HashMap<String, wgpu::ShaderModule>,
    pipelines: HashMap<String, EffectPipeline>,
    sampler: wgpu::Sampler,
    /// Fallback white mask, black texture.
    pub white_mask: wgpu::TextureView,
    pub black_tex: wgpu::TextureView,
    pub dummy_tex: wgpu::TextureView,
}

impl EffectCompiler {
    pub fn new(gpu: &Gpu) -> Self {
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effect-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (white_mask, black_tex, dummy_tex) = (
            solid_view(gpu, wgpu::TextureFormat::R8Unorm, &[255]),
            solid_view_rgba(gpu, wgpu::TextureFormat::Rgba8Unorm, &[0, 0, 0, 0]),
            solid_view_rgba(gpu, wgpu::TextureFormat::Rgba8Unorm, &[0, 0, 0, 0]),
        );
        Self {
            modules: HashMap::new(),
            pipelines: HashMap::new(),
            sampler,
            white_mask,
            black_tex,
            dummy_tex,
        }
    }

    /// The shared sampler.
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Compile (or reuse) the pipeline for a manifest.
    pub fn ensure_pipeline(
        &mut self,
        gpu: &Gpu,
        entry: &flux_catalog::CatalogEntry,
    ) -> FluxResult<()> {
        let id = &entry.manifest.id;
        if self.pipelines.contains_key(id) {
            return Ok(());
        }
        if entry.manifest.implementation.kind != "fragment" {
            // CPU nodes have a presentation shader; treat them as pass-through.
            return self.compile_manifest(gpu, entry, ShaderVariant::Present);
        }
        // CPU/scene rows present already-rendered textures.
        let is_present = entry
            .source
            .contains("textureSampleLevel(src_tex, src_sampler, uv, 0.0)\n  return")
            && entry.source.matches("fn process").count() == 1
            && entry
                .manifest
                .inputs
                .iter()
                .all(|p| p.id != "source" || entry.manifest.generator_id.is_empty() == false);
        let variant = if is_present {
            ShaderVariant::Present
        } else {
            ShaderVariant::Process
        };
        self.compile_manifest(gpu, entry, variant)
    }

    fn compile_manifest(
        &mut self,
        gpu: &Gpu,
        entry: &flux_catalog::CatalogEntry,
        variant: ShaderVariant,
    ) -> FluxResult<()> {
        let manifest = &entry.manifest;
        let generated = match variant {
            ShaderVariant::Process => shader_gen::generate_with(
                entry,
                if manifest
                    .inputs
                    .iter()
                    .any(|p| p.id == "source" && p.required)
                {
                    shader_gen::ShaderSpec::effect()
                } else {
                    shader_gen::ShaderSpec::generator()
                },
            ),
            ShaderVariant::Present => {
                shader_gen::generate_with(entry, shader_gen::ShaderSpec::generator())
            }
        };
        let source = if matches!(variant, ShaderVariant::Present) {
            // Ensure the present body exists (CPU/scene rows already carry it).
            generated
        } else {
            generated
        };
        // Validate the WGSL with Naga first: `create_shader_module` aborts the
        // process on a parse error, so we must catch it before it reaches wgpu.
        validate_wgsl(&manifest.id, &source)?;
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("effect-{}", manifest.id)),
                source: wgpu::ShaderSource::Wgsl(source.clone().into()),
            });
        gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let bgl = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("effect-bgl"),
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
        let input_layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("effect-input-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("effect-layout"),
                bind_group_layouts: &[&bgl, &input_layout],
                push_constant_ranges: &[],
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("effect-pipeline-{}", manifest.id)),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
        let uniform_size = {
            let (_, layout) = flux_catalog::pack::params_wgsl(&manifest.parameters);
            layout.byte_size().max(16)
        };
        if let Some(err) = pollster::block_on(gpu.device.pop_error_scope()) {
            return Err(FluxError::new(
                ErrorCode::ShaderCompile,
                "render",
                format!("{}: {err}", manifest.id),
            ));
        }
        self.pipelines.insert(
            manifest.id.clone(),
            EffectPipeline {
                type_id: manifest.id.clone(),
                bind_group_layout: gpu.device.create_bind_group_layout(
                    &wgpu::BindGroupLayoutDescriptor {
                        label: Some("effect-bgl-stored"),
                        entries: &[],
                    },
                ),
                pipeline,
                uniform_size,
                has_extra_inputs: manifest.inputs.len() > 2,
            },
        );
        Ok(())
    }

    pub fn pipeline(&self, type_id: &str) -> Option<&EffectPipeline> {
        self.pipelines.get(type_id)
    }
}

enum ShaderVariant {
    Process,
    Present,
}

/// Parse + validate WGSL with Naga and translate it to a Naga module. Returns a
/// readable diagnostic on failure instead of aborting the process (wgpu aborts
/// on shader parse errors).
pub fn validate_wgsl(label: &str, src: &str) -> FluxResult<()> {
    let module = naga::front::wgsl::parse_str(src).map_err(|e| {
        FluxError::new(
            ErrorCode::ShaderCompile,
            "render",
            format!("{label}: {}", e.emit_to_string(src)),
        )
    })?;
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    validator.validate(&module).map_err(|e| {
        FluxError::new(
            ErrorCode::ShaderCompile,
            "render",
            format!("{label}: {e:?}"),
        )
    })?;
    Ok(())
}

fn solid_view(gpu: &Gpu, format: wgpu::TextureFormat, value: &[u8]) -> wgpu::TextureView {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solid"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
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
        value,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(value.len() as u32),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    tex.create_view(&Default::default())
}

fn solid_view_rgba(gpu: &Gpu, format: wgpu::TextureFormat, value: &[u8]) -> wgpu::TextureView {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solid-rgba"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
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
        value,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(value.len() as u32),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    tex.create_view(&Default::default())
}

/// Helper that creates an intermediate render target texture + view.
pub fn render_target(gpu: &Gpu, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("effect-target"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    (tex, view)
}
