use crate::model::{Vertex, player_parts};
use crate::{Cape, Model, OrbitControls, Result, Skin, auto_rotate_yaw, invalid, walking_pose};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::{sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Clone, Copy, Debug)]
pub struct RenderSize {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

impl Default for RenderSize {
    fn default() -> Self {
        Self {
            width: 220,
            height: 300,
            scale_factor: 1.0,
        }
    }
}

impl RenderSize {
    pub fn physical_size(self) -> Result<(u32, u32)> {
        if self.width == 0
            || self.height == 0
            || !self.scale_factor.is_finite()
            || self.scale_factor <= 0.0
        {
            return Err(invalid(
                "logical size and scale factor must be positive and finite",
            ));
        }
        let w = (self.width as f64 * self.scale_factor as f64).floor();
        let h = (self.height as f64 * self.scale_factor as f64).floor();
        if !(1.0..=4096.0).contains(&w) || !(1.0..=4096.0).contains(&h) {
            return Err(invalid(
                "physical dimensions must be between 1 and 4096 pixels",
            ));
        }
        Ok((w as u32, h as u32))
    }
}

/// Row-major, top-left origin, tightly packed **straight-alpha** RGBA8 for Slint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
}

/// No window, surface, timer, worker or redraw loop. Each call submits one frame
/// and waits (at most 10 seconds) for GPU readback. Drop releases its GPU objects.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    size: RenderSize,
    output: wgpu::Texture,
    scene_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    row_pitch: u32,
    uniforms: wgpu::Buffer,
    vertices: wgpu::Buffer,
    skin: wgpu::BindGroup,
    cape: Option<wgpu::BindGroup>,
    model: Model,
    opaque: [wgpu::RenderPipeline; 2],
    transparent: [[wgpu::RenderPipeline; 2]; 2],
    fxaa: wgpu::RenderPipeline,
    fxaa_bind: wgpu::BindGroup,
    pub controls: OrbitControls,
}

impl Renderer {
    /// `software=true` requires a fallback adapter (e.g. Vulkan lavapipe).
    /// Does not create a surface or consult DISPLAY/WAYLAND_DISPLAY.
    pub async fn new(
        size: RenderSize,
        skin: &Skin,
        cape: Option<&Cape>,
        software: bool,
    ) -> Result<Self> {
        let (width, height) = size.physical_size()?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: software,
            })
            .await?;
        let adapter_info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("skin3d offscreen"),
                ..Default::default()
            })
            .await?;
        let texture = |name: &str, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(name),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let scene = texture(
            "skin3d linear scene",
            FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let scene_view = scene.create_view(&Default::default());
        let output = texture(
            "skin3d FXAA output",
            FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth_view = texture(
            "skin3d depth",
            DEPTH,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());
        let row_pitch = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skin3d readback"),
            size: u64::from(row_pitch) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skin3d camera"),
            contents: bytemuck::bytes_of(&Uniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skin3d vertices"),
            size: (13 * 36 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin3d player layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(1),
                sampler_entry(2),
            ],
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest skin/cape"),
            ..Default::default()
        });
        let skin_bind = texture_bind(
            &device,
            &queue,
            &bind_layout,
            &uniforms,
            &nearest,
            &skin.pixels,
        );
        let cape_bind = cape.map(|c| {
            texture_bind(
                &device,
                &queue,
                &bind_layout,
                &uniforms,
                &nearest,
                &c.pixels,
            )
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skin3d pipeline layout"),
            bind_group_layouts: &[&bind_layout],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("player.wgsl"));
        let opaque = [false, true].map(|biased| {
            player_pipeline(&device, &layout, &shader, false, biased, wgpu::Face::Back)
        });
        let transparent = [false, true].map(|biased| {
            [wgpu::Face::Front, wgpu::Face::Back]
                .map(|cull| player_pipeline(&device, &layout, &shader, true, biased, cull))
        });
        let fxaa_shader = device.create_shader_module(wgpu::include_wgsl!("fxaa.wgsl"));
        let fxaa_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin3d FXAA layout"),
            entries: &[texture_entry(0), sampler_entry(1)],
        });
        let fxaa_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skin3d FXAA pipeline layout"),
            bind_group_layouts: &[&fxaa_layout],
            immediate_size: 0,
        });
        let fxaa = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skin3d FXAA"),
            layout: Some(&fxaa_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &fxaa_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fxaa_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("FXAA linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let fxaa_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skin3d FXAA inputs"),
            layout: &fxaa_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&linear),
                },
            ],
        });
        Ok(Self {
            device,
            queue,
            adapter_info,
            size,
            output,
            scene_view,
            depth_view,
            readback,
            row_pitch,
            uniforms,
            vertices,
            skin: skin_bind,
            cape: cape_bind,
            model: skin.model(),
            opaque,
            transparent,
            fxaa,
            fxaa_bind,
            controls: OrbitControls::default(),
        })
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// No clock accumulation: the same time, offsets and drag history give the
    /// same frame on the same adapter. `yaw_drag_offset` is camera azimuth, radians.
    pub fn render_frame(&mut self, t_seconds: f64, yaw_drag_offset: f32) -> Result<Frame> {
        self.render_frame_with_pose_time(t_seconds, t_seconds, yaw_drag_offset)
    }

    pub(crate) fn render_frame_with_pose_time(
        &mut self,
        t_seconds: f64,
        pose_seconds: f64,
        yaw_drag_offset: f32,
    ) -> Result<Frame> {
        if !pose_seconds.is_finite() || pose_seconds < 0.0 || pose_seconds > t_seconds {
            return Err(invalid(
                "pose time must be finite and between zero and frame time",
            ));
        }
        let rotation_time = self.controls.rotation_time(t_seconds)?;
        if !yaw_drag_offset.is_finite() {
            return Err(invalid("yaw offset must be finite"));
        }
        let distance = (4.5 + 16.5 / (50.0_f32.to_radians() / 2.0).tan() / 0.85).clamp(10.0, 256.0);
        let theta = self.controls.yaw + yaw_drag_offset.rem_euclid(std::f32::consts::TAU);
        let elevation = self.controls.elevation;
        let camera = Vec3::new(
            theta.sin() * elevation.cos(),
            elevation.sin(),
            theta.cos() * elevation.cos(),
        ) * distance;
        let view = glam::camera::rh::view::look_at_mat4(camera, Vec3::ZERO, Vec3::Y);
        let projection = glam::camera::rh::proj::directx::perspective(
            50.0_f32.to_radians(),
            self.size.width as f32 / self.size.height as f32,
            0.1,
            2000.0,
        );
        self.queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&Uniforms {
                view_projection: (projection * view).to_cols_array_2d(),
                camera: camera.extend(0.0).to_array(),
            }),
        );
        let mut parts = player_parts(
            self.model,
            self.cape.is_some(),
            walking_pose(pose_seconds),
            auto_rotate_yaw(rotation_time),
        );
        // Three sorts transparent meshes back-to-front by projected object center.
        parts.sort_by(|a, b| {
            a.outer.cmp(&b.outer).then_with(|| {
                view.transform_point3(a.center)
                    .z
                    .total_cmp(&view.transform_point3(b.center).z)
            })
        });
        let vertices: Vec<_> = parts
            .iter()
            .flat_map(|p| p.vertices.iter().copied())
            .collect();
        self.queue
            .write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let attachments = [Some(colour_attachment(&self.scene_view))];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("skin3d player"),
                color_attachments: &attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            for (i, part) in parts.iter().enumerate() {
                pass.set_bind_group(
                    0,
                    if part.cape {
                        self.cape.as_ref().expect("cape mesh has texture")
                    } else {
                        &self.skin
                    },
                    &[],
                );
                let range = (i * 36) as u32..((i + 1) * 36) as u32;
                if part.outer {
                    // Three renders transparent DoubleSide back faces, then front faces.
                    for pipeline in &self.transparent[usize::from(part.biased)] {
                        pass.set_pipeline(pipeline);
                        pass.draw(range.clone(), 0..1);
                    }
                } else {
                    pass.set_pipeline(&self.opaque[usize::from(part.biased)]);
                    pass.draw(range, 0..1);
                }
            }
        }
        let output_view = self.output.create_view(&Default::default());
        {
            let attachments = [Some(colour_attachment(&output_view))];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("skin3d FXAA"),
                color_attachments: &attachments,
                ..Default::default()
            });
            pass.set_pipeline(&self.fxaa);
            pass.set_bind_group(0, &self.fxaa_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        let (width, height) = self.size.physical_size()?;
        encoder.copy_texture_to_buffer(
            self.output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.row_pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::sync_channel(1);
        self.readback
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = sender.send(result);
            });
        if let Err(error) = self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        }) {
            self.readback.unmap();
            return Err(error.into());
        }
        let mapping = receiver
            .recv_timeout(Duration::from_secs(1))
            .map_err(crate::Error::from)
            .and_then(|result| result.map_err(crate::Error::from));
        if let Err(error) = mapping {
            self.readback.unmap();
            return Err(error);
        }
        let mapped = self.readback.get_mapped_range(..);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks_exact(self.row_pitch as usize) {
            rgba.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(mapped);
        self.readback.unmap();
        // Render targets contain premultiplied color after normal alpha blending/FXAA.
        // Slint Image::from_rgba8 expects straight alpha; unpremultiply exactly once.
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = (u32::from(*channel) * 255 + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
        }
        Ok(Frame {
            width,
            height,
            rgba,
        })
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn texture_bind(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    pixels: &image::RgbaImage,
) -> wgpu::BindGroup {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("skin3d nearest PNG"),
        size: wgpu::Extent3d {
            width: pixels.width(),
            height: pixels.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        pixels.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(pixels.width() * 4),
            rows_per_image: Some(pixels.height()),
        },
        texture.size(),
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("skin3d player inputs"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &texture.create_view(&Default::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn player_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    outer: bool,
    biased: bool,
    cull: wgpu::Face,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("skin3d player material"), layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[
            wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2] }
        ] },
        primitive: wgpu::PrimitiveState { cull_mode: Some(cull), ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: Default::default(), bias: if biased { wgpu::DepthBiasState { constant: 1, slope_scale: 1.0, clamp: 0.0 } } else { Default::default() } }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some(if outer { "outer" } else { "inner" }), compilation_options: Default::default(), targets: &[
            Some(wgpu::ColorTargetState { format: FORMAT, blend: outer.then_some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })
        ] }),
        multiview_mask: None, cache: None,
    })
}

fn colour_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_dimensions_and_input_validation() {
        for (scale_factor, expected) in [(1.0, (220, 300)), (1.25, (275, 375)), (1.5, (330, 450))] {
            assert_eq!(
                RenderSize {
                    scale_factor,
                    ..Default::default()
                }
                .physical_size()
                .unwrap(),
                expected
            );
        }
        for scale_factor in [0.0, -1.0, f32::NAN, f32::INFINITY, 100.0] {
            assert!(
                RenderSize {
                    scale_factor,
                    ..Default::default()
                }
                .physical_size()
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod phase_tests {
    use super::*;

    #[test]
    #[ignore = "requires a headless wgpu software adapter"]
    fn walking_lags_rotation_by_the_previous_delivered_frame() {
        let skin =
            Skin::from_png(include_bytes!("../fixtures/uv-skin.png"), Model::Classic).unwrap();
        let mut renderer =
            pollster::block_on(Renderer::new(RenderSize::default(), &skin, None, true)).unwrap();
        let current = renderer.render_frame(1.0, 0.0).unwrap();
        let lagged = renderer.render_frame_with_pose_time(1.0, 0.9, 0.0).unwrap();
        assert_ne!(current, lagged);
        assert_eq!(
            lagged,
            renderer.render_frame_with_pose_time(1.0, 0.9, 0.0).unwrap()
        );
        assert_eq!(
            current,
            renderer.render_frame_with_pose_time(1.0, 1.0, 0.0).unwrap()
        );
        for invalid in [f64::NAN, -0.1, 1.1] {
            assert!(
                renderer
                    .render_frame_with_pose_time(1.0, invalid, 0.0)
                    .is_err()
            );
        }
    }
}
