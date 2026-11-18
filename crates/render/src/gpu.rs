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

/// Process-wide serialiser around GPU *device creation*.
///
/// [`Gpu::new`] is the only path in this workspace that builds a
/// `wgpu::Instance`, enumerates adapters and blocks on `request_device`, so it is
/// the one place the guarantee can actually be made. Until now the guarantee was
/// declared per test file instead: `crates/tests/tests/dag_parallel.rs` holds a
/// function-local mutex ("GPU device creation is not safe concurrently from
/// multiple test threads (it can deadlock)" — D045 calls it the repo's only
/// mechanism note), which covers that file's four tests and nothing else. Every
/// other device caller was covered by whichever lock its own binary happened to
/// take: `flux-render`'s lib-suite and `flux-app`'s by none, which is why
/// `--test-threads=1` was load-bearing rather than hygienic.
///
/// Two things this deliberately does not claim. It is not justified by a
/// reproduction — D045 ran twelve parallel live-audio legs at `efdf253` and none
/// hung, so the fault stays unreproduced and bounded at roughly one run in five.
/// And it guards creation only: the lock is released before any submit, render or
/// readback, so it serialises one device init per `Gpu`, never a frame.
fn device_creation_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    L.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        // A holder that panicked mid-init left no state worth preserving, and
        // propagating the poison would turn one crash into every later GPU test
        // failing for a reason that names neither the crash nor the device.
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
    ///
    /// Takes [`device_creation_lock`] for the duration of init; see the comment
    /// there for what that does and does not buy.
    pub fn new(backend: BackendPref) -> FluxResult<Self> {
        let _serialiser = device_creation_lock();
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
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// The serialiser's whole contract in one assertion: a caller cannot get a
    /// device while somebody else is creating one.
    ///
    /// This is the falsifier for the guard line in [`Gpu::new`] — delete it and
    /// the worker answers inside the hold window, which is the failure this test
    /// is named for. It needs no working GPU: the worker only has to *reach*
    /// `Gpu::new` and be kept out of it, and its result (device or
    /// `GpuDeviceLost`) is reported either way.
    #[test]
    fn device_creation_waits_for_the_process_serialiser() {
        // Calibrate before asserting. A fixed hold window shorter than a real
        // unlocked creation is a gate that passes on the broken artifact — the
        // quiet direction of failure — and this host measured one unlocked
        // `Gpu::new` at 674 ms with two hosted CI runs in flight. The window is
        // therefore derived from the machine's own number, not chosen.
        let started = Instant::now();
        let _baseline = Gpu::new(BackendPref::Auto);
        let unlocked = started.elapsed();
        let hold = unlocked * 3 + Duration::from_millis(250);

        let held = device_creation_lock();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let outcome = Gpu::new(BackendPref::Auto).is_ok();
            // Send the verdict, not the device: the worker's `Gpu` drops here.
            tx.send(outcome).is_ok()
        });

        let answered = rx.recv_timeout(hold).ok();
        assert!(
            answered.is_none(),
            "Gpu::new answered inside a {} ms hold window (this machine creates a \
             device in {} ms) while the process serialiser was held — the guard \
             line in Gpu::new is gone",
            hold.as_millis(),
            unlocked.as_millis()
        );

        drop(held);
        let after_release = Instant::now();
        assert!(
            rx.recv_timeout(RELEASE_LIMIT).is_ok(),
            "Gpu::new never returned after the serialiser was released"
        );
        assert!(
            worker.join().is_ok(),
            "the device-creation worker panicked rather than returning"
        );
        eprintln!(
            "serialiser control: unlocked creation {unlocked:?}, held for {} ms, \
             worker answered {} ms after release",
            hold.as_millis(),
            after_release.elapsed().as_millis()
        );
    }

    /// Creation is a one-shot driver init, not a render loop, so a minute and a
    /// half after release means "this never came back" rather than "slow".
    const RELEASE_LIMIT: Duration = Duration::from_millis(90_000);

    /// One `Instance::new` in this file, and it is behind the lock. A second,
    /// unlocked construction site is exactly the defect the process-wide lock is
    /// supposed to make impossible, and nothing else in the repo can see it.
    ///
    /// The needle is `concat!`, not a literal, on purpose: this test lives in the
    /// file it reads, so spelling the searched-for text is how the first version
    /// of this guard counted three occurrences of one call site (measured:
    /// `left: 3` — the extra two were this test's own source lines).
    #[test]
    fn the_only_instance_construction_in_this_file_is_the_locked_one() {
        let src = include_str!("gpu.rs");
        let needle = concat!("wgpu::Instance", "::new");
        assert_eq!(
            src.matches(needle).count(),
            1,
            "a second device-creation site appeared in gpu.rs, outside the one \
             the serialiser guards"
        );
        let at = src.find(needle).unwrap();
        let between = &src[src.find("pub fn new(backend: BackendPref)").unwrap()..at];
        assert!(
            between.contains("let _serialiser = device_creation_lock();"),
            "the guard line is not between Gpu::new's entry and the instance it builds"
        );
    }

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
