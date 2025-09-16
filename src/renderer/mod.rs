use wgpu::*;
use wgpu::util::DeviceExt;
use nalgebra as na;
use bytemuck::{Pod, Zeroable};
use log::{info, debug};
use std::time::Instant;

pub mod compute_pipeline;
pub mod performance;
pub mod blit_pipeline;

use compute_pipeline::ComputePipeline;
use performance::PerformanceController;
use blit_pipeline::BlitPipeline;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraData {
    pub view_proj: [[f32; 4]; 4],
    pub position: [f32; 3],
    pub _padding1: f32,
    pub forward: [f32; 3],
    pub _padding2: f32,
    pub screen_size: [f32; 2],
    pub _padding3: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PerformanceData {
    pub base_voxel_size: f32,
    pub frame_time: f32,
    pub _padding: [f32; 2],
}

pub struct VoxelRenderer {
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    compute_pipeline: ComputePipeline,
    blit_pipeline: BlitPipeline,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    performance_buffer: Buffer,
    performance_bind_group: BindGroup,
    performance_controller: PerformanceController,
    output_texture: Texture,
    output_texture_view: TextureView,
    last_frame_time: Instant,
    frame_count: u32,
}

impl VoxelRenderer {
    pub fn new(
        device: &Device,
        _queue: &Queue,
        adapter: &Adapter,
        surface: Surface<'static>,
        width: u32,
        height: u32,
    ) -> Self {
        info!("Creating VoxelRenderer with resolution {}x{}", width, height);

        let surface_caps = surface.get_capabilities(adapter);
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(device, &surface_config);

        // Create camera uniform buffer
        let camera_data = Self::create_camera_data(width as f32, height as f32);
        let camera_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_data]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
            ],
        });

        // Create performance uniform buffer
        let performance_data = PerformanceData {
            base_voxel_size: 1.0,
            frame_time: 0.016,
            _padding: [0.0; 2],
        };

        let performance_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Performance Buffer"),
            contents: bytemuck::cast_slice(&[performance_data]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let performance_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Performance Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let performance_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Performance Bind Group"),
            layout: &performance_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: performance_buffer.as_entire_binding(),
                },
            ],
        });

        // Create compute pipeline
        let compute_pipeline = ComputePipeline::new(
            device,
            &camera_bind_group_layout,
            &performance_bind_group_layout,
        );

        // Create blit pipeline for format conversion
        let blit_pipeline = BlitPipeline::new(device, surface_format);

        // Create output texture for compute shader
        let output_texture = device.create_texture(&TextureDescriptor {
            label: Some("Compute Output Texture"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_texture_view = output_texture.create_view(&TextureViewDescriptor::default());

        // Create performance controller
        let performance_controller = PerformanceController::new(20.0);

        Self {
            surface,
            surface_config,
            compute_pipeline,
            blit_pipeline,
            camera_buffer,
            camera_bind_group,
            performance_buffer,
            performance_bind_group,
            performance_controller,
            output_texture,
            output_texture_view,
            last_frame_time: Instant::now(),
            frame_count: 0,
        }
    }

    pub fn resize(&mut self, device: &Device, queue: &Queue, width: u32, height: u32) {
        if width > 0 && height > 0 {
            info!("Resizing to {}x{}", width, height);
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(device, &self.surface_config);

            // Recreate output texture
            self.output_texture = device.create_texture(&TextureDescriptor {
                label: Some("Compute Output Texture"),
                size: Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.output_texture_view = self.output_texture.create_view(&TextureViewDescriptor::default());

            // Update camera data
            let camera_data = Self::create_camera_data(width as f32, height as f32);
            queue.write_buffer(
                &self.camera_buffer,
                0,
                bytemuck::cast_slice(&[camera_data]),
            );
        }
    }

    pub fn render(&mut self, device: &Device, queue: &Queue) {
        let now = Instant::now();
        let delta_time = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;

        // Update performance controller
        if let Some(new_voxel_size) = self.performance_controller.update(delta_time) {
            debug!("Adjusting base voxel size to: {}", new_voxel_size);
            let performance_data = PerformanceData {
                base_voxel_size: new_voxel_size,
                frame_time: delta_time,
                _padding: [0.0; 2],
            };
            queue.write_buffer(
                &self.performance_buffer,
                0,
                bytemuck::cast_slice(&[performance_data]),
            );
        }

        let output = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(e) => {
                log::error!("Failed to get current texture: {:?}", e);
                return;
            }
        };

        let surface_view = output.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Run compute shader on our storage texture
        self.compute_pipeline.dispatch(
            device,
            &mut encoder,
            &self.output_texture_view,
            &self.camera_bind_group,
            &self.performance_bind_group,
            self.surface_config.width,
            self.surface_config.height,
        );

        // Blit from compute output to surface (handles format conversion)
        self.blit_pipeline.blit(
            device,
            &mut encoder,
            &self.output_texture_view,
            &surface_view,
        );

        queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.frame_count += 1;
        if self.frame_count % 60 == 0 {
            info!("Frame time: {:.2}ms, FPS: {:.1}", delta_time * 1000.0, 1.0 / delta_time);
        }
    }

    fn create_camera_data(width: f32, height: f32) -> CameraData {
        let aspect_ratio = width / height;
        let fov_y = 45.0_f32.to_radians();
        let near = 0.1;
        let far = 1000.0;

        // Camera positioned to look into Cornell Box (same as screenshot default)
        let eye = na::Point3::new(0.0, 1.0, -2.5);
        let target = na::Point3::new(0.0, 1.0, 1.0);
        let up = na::Vector3::new(0.0, 1.0, 0.0);

        let view = na::Matrix4::look_at_rh(&eye, &target, &up);
        let proj = na::Matrix4::new_perspective(aspect_ratio, fov_y, near, far);
        let view_proj = proj * view;

        let forward = (target - eye).normalize();

        CameraData {
            view_proj: view_proj.into(),
            position: [eye.x, eye.y, eye.z],
            _padding1: 0.0,
            forward: [forward.x, forward.y, forward.z],
            _padding2: 0.0,
            screen_size: [width, height],
            _padding3: [0.0; 2],
        }
    }
}