//! wgpu GPU context: adapter/device selection, capability reporting, and a
//! headless render target. The same code path serves the editor and export;
//! the only difference is where the final texture is read or presented.

use flux_domain::{ErrorCode, FluxError, FluxResult};
use std::sync::Arc;

/// Backend selection preference from settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendPref {
    #[default]
    Auto,
    Dx12,
    Vulkan,
    Metal,
    Gl,
}

impl BackendPref {
    pub fn backend(self) -> Option<wgpu::Backends> {
        match self {
            BackendPref::Auto => None,
            BackendPref::Dx12 => Some(wgpu::Backends::DX12),
            BackendPref::Vulkan => Some(wgpu::Backends::VULKAN),
            BackendPref::Metal => Some(wgpu::Backends::METAL),
            BackendPref::Gl => Some(wgpu::Backends::GL),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub max_texture_2d: u32,
    pub supports_rgba16f_filter: bool,
    pub max_uniform_binding_size: u32,
}

impl GpuInfo {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "backend": self.backend,
            "deviceType": self.device_type,
            "driver": self.driver,
            "maxTexture2D": self.max_texture_2d,
            "supportsRgba16fFilter": self.supports_rgba16f_filter,
            "maxUniformBindingSize": self.max_uniform_binding_size,
        })
    }
}

pub struct Gpu {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub info: GpuInfo,
    pub adapter: wgpu::Adapter,
}

impl Gpu {
    /// Create a GPU context. `backend` selects a preferred backend; `Auto`
    /// prefers high-performance (discrete) adapters with DX12/Vulkan/Metal.
    pub fn new(backend: BackendPref) -> FluxResult<Self> {
        let mut instance_desc = wgpu::InstanceDescriptor::default();
        if let Some(b) = backend.backend() {
            instance_desc.backends = b;
        }
        let instance = wgpu::Instance::new(&instance_desc);
        let adapters = instance.enumerate_adapters(instance_desc.backends);
        // Prefer discrete adapters, then integrated, then anything.
        let mut ranked: Vec<wgpu::Adapter> = adapters;
        ranked.sort_by_key(|a| {
            let info = a.get_info();
            match info.device_type {
                wgpu::DeviceType::DiscreteGpu => 0,
                wgpu::DeviceType::IntegratedGpu => 1,
                wgpu::DeviceType::VirtualGpu => 2,
                _ => 3,
            }
        });
        let Some(adapter) = ranked.into_iter().next() else {
            return Err(FluxError::new(
                ErrorCode::GpuDeviceLost,
                "render",
                "no GPU adapter found",
            ));
        };
        let info = adapter.get_info();
        let limits = wgpu::Limits::default();
        let features = wgpu::Features::empty();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("flux-device"),
            required_features: features,
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", e.to_string()))?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let caps = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let d = device.limits();
        let gpu_info = GpuInfo {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: format!("{} {}", info.driver, info.driver_info),
            max_texture_2d: d.max_texture_dimension_2d,
            supports_rgba16f_filter: caps
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE),
            max_uniform_binding_size: d.max_uniform_buffer_binding_size,
        };
        Ok(Self {
            device,
            queue,
            info: gpu_info,
            adapter,
        })
    }

    /// Render to a headless RGBA8 target and read it back, applying a synchronous
    /// device poll. Returns tightly packed RGBA (row stride = width*4).
    pub fn render_offscreen(
        &self,
        width: u32,
        height: u32,
        draw: impl FnOnce(&wgpu::Device, &wgpu::Queue, &wgpu::Texture, &mut wgpu::CommandEncoder),
    ) -> FluxResult<Vec<u8>> {
        let (w, h) = (width.max(1), height.max(1));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let padded = padded_bytes_per_row(w, 4);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded as u64) * (h as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("offscreen-encoder"),
            });
        draw(&self.device, &self.queue, &texture, &mut encoder);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|e| FluxError::new(ErrorCode::GpuDeviceLost, "render", e.to_string()))?;
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
        buffer.unmap();
        Ok(out)
    }
}

/// 256-byte row-alignment padding used for readback copies.
pub fn padded_bytes_per_row(width: u32, bytes_pp: u32) -> u32 {
    let unpadded = width * bytes_pp;
    let align = 256u32;
    unpadded.div_ceil(align) * align
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_rounds_up_to_256() {
        assert_eq!(padded_bytes_per_row(64, 4), 256);
        assert_eq!(padded_bytes_per_row(100, 4), 512); // 400 -> 512
        assert_eq!(padded_bytes_per_row(1920, 4), 1920 * 4);
    }

    #[test]
    fn gpu_available_reports_adapter() {
        match Gpu::new(BackendPref::Auto) {
            Ok(gpu) => {
                assert!(!gpu.info.name.is_empty());
                assert!(gpu.info.max_texture_2d >= 2048);
            }
            Err(e) => panic!("GPU required for tests: {e}"),
        }
    }
}
