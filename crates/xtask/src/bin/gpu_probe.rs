use flux_catalog::build_catalog;
use flux_render::pipeline::{EffectCompiler, NodeParamsBuffer};
use flux_render::{BackendPref, Gpu};

fn main() {
    let g = Gpu::new(BackendPref::Auto).unwrap();
    let catalog = build_catalog();
    let mut compiler = EffectCompiler::new(&g);
    let entry = catalog.get("gen.solid").unwrap();
    compiler.ensure_pipeline(&g, entry).unwrap();
    let pipeline = compiler.pipeline("gen.solid").unwrap();

    // frame uniform
    let frame_buf = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 96,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut f = [0u8; 96];
    // dims (u32 x4) at 0
    for (i, v) in [64u32, 64, 64, 64].iter().enumerate() {
        f[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in [1.0f32 / 64.0, 1.0 / 64.0, 1.0, 1.0].iter().enumerate() {
        let o = 16 + i * 4;
        f[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    // controls wet at offset 80
    f[80..84].copy_from_slice(&1.0f32.to_le_bytes());
    g.queue.write_buffer(&frame_buf, 0, &f);

    let np_bytes = NodeParamsBuffer { floats: vec![[1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0]], uints: vec![[0u32; 4]] }.bytes();
    let np_buf = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    g.queue.write_buffer(&np_buf, 0, &np_bytes);
    let audio_buf = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 48,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    g.queue.write_buffer(&audio_buf, 0, &[0u8; 48]);

    let bgl0 = g.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry_buf(0, Some(96)),
            entry_buf(1, None),
            entry_buf(2, Some(48)),
        ],
    });
    let bg0 = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl0,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: np_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: audio_buf.as_entire_binding() },
        ],
    });
    let bgl1 = g.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[tex_entry(0), tex_entry(1), wgpu::BindGroupLayoutEntry {
            binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }],
    });
    let bg1 = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl1,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&compiler.white_mask) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&compiler.white_mask) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(compiler.sampler()) },
        ],
    });

    let px = g
        .render_offscreen(64, 64, |_d, _q, tex, enc| {
            let view = tex.create_view(&Default::default());
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&pipeline.pipeline);
            rp.set_bind_group(0, &bg0, &[]);
            rp.set_bind_group(1, &bg1, &[]);
            rp.draw(0..3, 0..1);
        })
        .unwrap();
    println!("center pixel {:?}", &px[((32 * 64 + 32) * 4)..((32 * 64 + 32) * 4) + 4]);
}

fn entry_buf(binding: u32, min: Option<u64>) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: min.and_then(wgpu::BufferSize::new),
        },
        count: None,
    }
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
