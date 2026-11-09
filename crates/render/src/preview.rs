//! Bounded asynchronous preview: downscale the authoritative output into a
//! small RGBA buffer for the editor. Never reads back the native output path.

use crate::gpu::Gpu;
use flux_domain::{ErrorCode, FluxError, FluxResult};

pub const PREVIEW_MAX_W: u32 = 960;
pub const PREVIEW_MAX_H: u32 = 540;

/// Downscale to fit the preview box, preserving aspect ratio, never upscaling.
pub fn preview_size(width: u32, height: u32) -> (u32, u32) {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let scale = (PREVIEW_MAX_W as f32 / w).min(PREVIEW_MAX_H as f32 / h).min(1.0);
    (((w * scale) as u32).max(1), ((h * scale) as u32).max(1))
}

/// Read the target texture back at preview resolution using the GPU's own
/// blit-free readback of a downsampled copy. The copy is done with a
/// nearest-sample pass into a small texture, then mapped.
pub fn read_preview(
    gpu: &Gpu,
    source: &wgpu::Texture,
    src_w: u32,
    src_h: u32,
) -> FluxResult<(u32, u32, Vec<u8>)> {
    let (w, h) = preview_size(src_w, src_h);
    // Use a single pass fullscreen triangle to sample the source with linear
    // filtering into an RGBA8 target, then read it back.
    let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("preview-blit"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VOut {
  var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -3.0), vec2<f32>(-1.0, 1.0), vec2<f32>(3.0, 1.0));
  var o: VOut;
  o.pos = vec4<f32>(p[idx], 0.0, 1.0);
  o.uv = p[idx] * vec2<f32>(0.5, 0.5) + vec2<f32>(0.5, 0.5);
  return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
  return textureSample(src_tex, samp, in.uv);
}
"#
            .into(),
        ),
    });
    let bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("preview-bgl"),
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
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("preview-layout"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("preview-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });
    let src_view = source.create_view(&Default::default());
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("preview-bind"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });
    let dst = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview-dst"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let dst_view = dst.create_view(&Default::default());
    let padded = crate::gpu::padded_bytes_per_row(w, 4);
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("preview-readback"),
        size: padded as u64 * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("preview") });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("preview-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &dst_view,
                resolve_target: None, depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(&pipeline);
        rpass.set_bind_group(0, &bind, &[]);
        rpass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &dst,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", e.to_string()))?;
    rx.recv()
        .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", e.to_string()))?
        .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", format!("{e:?}")))?;
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    drop(data);
    buf.unmap();
    Ok((w, h, out))
}

/// Read the full-resolution output texture back as tightly packed RGBA8. Used
/// by the exporter; the editor uses `read_preview` (bounded downscale).
pub fn read_full(
    gpu: &Gpu,
    source: &wgpu::Texture,
    width: u32,
    height: u32,
) -> FluxResult<(u32, u32, Vec<u8>)> {
    let padded = crate::gpu::padded_bytes_per_row(width, 4);
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("full-readback"),
        size: padded as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("full-read") });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device
        .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
        .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", e.to_string()))?;
    rx.recv()
        .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", e.to_string()))?
        .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", format!("{e:?}")))?;
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    buf.unmap();
    Ok((width, height, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_size_limits() {
        assert_eq!(preview_size(1920, 1080), (960, 540));
        assert_eq!(preview_size(3840, 2160), (960, 540));
        assert_eq!(preview_size(640, 360), (640, 360));
        assert_eq!(preview_size(100, 1000), (54, 540));
    }
}
