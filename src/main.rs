use wgpu::*;
use wgpu::util::DeviceExt;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};
use env_logger;
use log::info;
use std::sync::Arc;
use clap::Parser;
use chrono::Local;

mod renderer;
use renderer::VoxelRenderer;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run in screenshot mode (render one frame and save to disk)
    #[arg(long)]
    screenshot: bool,

    /// Camera X position
    #[arg(long, default_value_t = 0.0)]
    cam_x: f32,

    /// Camera Y position
    #[arg(long, default_value_t = 1.0)]
    cam_y: f32,

    /// Camera Z position (negative is in front of the box)
    #[arg(long, default_value_t = -3.8)]
    cam_z: f32,

    /// Camera look-at X position
    #[arg(long, default_value_t = 0.0)]
    look_x: f32,

    /// Camera look-at Y position
    #[arg(long, default_value_t = 1.0)]
    look_y: f32,

    /// Camera look-at Z position
    #[arg(long, default_value_t = 1.0)]
    look_z: f32,

    /// Window width
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Window height
    #[arg(long, default_value_t = 720)]
    height: u32,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    pollster::block_on(run(args));
}

async fn run(args: Args) {
    info!("Starting Adaptive Voxel Path Tracer");

    if args.screenshot {
        info!("Screenshot mode enabled");
        run_screenshot_mode(args).await;
    } else {
        run_interactive_mode(args).await;
    }
}

async fn run_interactive_mode(args: Args) {
    let event_loop = EventLoop::new().unwrap();

    #[allow(deprecated)]
    let window = Arc::new(event_loop.create_window(Window::default_attributes()
        .with_title("Adaptive Voxel Path Tracer")
        .with_inner_size(winit::dpi::LogicalSize::new(args.width, args.height))).unwrap());

    // Initialize WebGPU
    let instance = Instance::new(&InstanceDescriptor {
        backends: Backends::all(),
        ..Default::default()
    });

    let surface = instance.create_surface(window.clone()).unwrap();

    let adapter = instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }).await.unwrap();

    info!("Using adapter: {:?}", adapter.get_info());

    let (device, queue) = adapter.request_device(
        &DeviceDescriptor::default(),
    ).await.unwrap();

    let size = window.inner_size();
    let mut renderer = VoxelRenderer::new(&device, &queue, &adapter, surface, size.width, size.height);

    #[allow(deprecated)]
    let _ = event_loop.run(move |event, control_flow| {
        match event {
            Event::WindowEvent {
                window_id,
                event
            } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => {
                        control_flow.exit();
                    }
                    WindowEvent::Resized(physical_size) => {
                        if physical_size.width > 0 && physical_size.height > 0 {
                            renderer.resize(&device, &queue, physical_size.width, physical_size.height);
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        renderer.render(&device, &queue);
                        window.request_redraw();
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}

async fn run_screenshot_mode(args: Args) {
    info!("Initializing headless screenshot renderer");

    // Initialize WebGPU without a window
    let instance = Instance::new(&InstanceDescriptor {
        backends: Backends::all(),
        ..Default::default()
    });

    // Request adapter
    let adapter = instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }).await.unwrap();

    info!("Using adapter: {:?}", adapter.get_info());

    let (device, queue) = adapter.request_device(
        &DeviceDescriptor::default(),
    ).await.unwrap();

    // Create a texture to render to
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("Screenshot Texture"),
        size: Extent3d {
            width: args.width,
            height: args.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    // Create buffer to read back the texture
    let buffer_size = (args.width * args.height * 4) as BufferAddress;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Screenshot Buffer"),
        size: buffer_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Render one frame with specified camera parameters
    let texture_view = texture.create_view(&TextureViewDescriptor::default());
    render_screenshot_frame(
        &device,
        &queue,
        &texture_view,
        &args,
        args.width,
        args.height,
    ).await;

    // Copy texture to buffer
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Screenshot Encoder"),
    });

    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(args.width * 4),
                rows_per_image: None,
            },
        },
        Extent3d {
            width: args.width,
            height: args.height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    // Read back the buffer
    let buffer_slice = buffer.slice(..);
    let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
    buffer_slice.map_async(MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });

    device.poll(wgpu::PollType::Wait).unwrap();
    rx.receive().await.unwrap().unwrap();

    let data = buffer_slice.get_mapped_range();

    // Save to file
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!(
        "screenshot_{}_cam_{:.1}_{:.1}_{:.1}_look_{:.1}_{:.1}_{:.1}_{}x{}.png",
        timestamp,
        args.cam_x, args.cam_y, args.cam_z,
        args.look_x, args.look_y, args.look_z,
        args.width, args.height
    );

    let mut image_data = vec![0u8; (args.width * args.height * 4) as usize];
    image_data.copy_from_slice(&data);
    drop(data);
    buffer.unmap();

    // No color channel swapping needed - format is already RGBA

    let image = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
        args.width,
        args.height,
        image_data,
    ).unwrap();

    image.save(&filename).unwrap();
    info!("Screenshot saved to: {}", filename);
}

async fn render_screenshot_frame(
    device: &Device,
    queue: &Queue,
    target: &TextureView,
    args: &Args,
    width: u32,
    height: u32,
) {
    use nalgebra as na;

    // Create camera data with specified parameters
    let eye = na::Point3::new(args.cam_x, args.cam_y, args.cam_z);
    let target_point = na::Point3::new(args.look_x, args.look_y, args.look_z);
    let up = na::Vector3::new(0.0, 1.0, 0.0);

    let aspect_ratio = width as f32 / height as f32;
    let fov_y = 45.0_f32.to_radians();
    let near = 0.1;
    let far = 1000.0;

    let view = na::Matrix4::look_at_rh(&eye, &target_point, &up);
    let proj = na::Matrix4::new_perspective(aspect_ratio, fov_y, near, far);
    let view_proj = proj * view;

    let forward = (target_point - eye).normalize();

    let camera_data = renderer::CameraData {
        view_proj: view_proj.into(),
        position: [eye.x, eye.y, eye.z],
        _padding1: 0.0,
        forward: [forward.x, forward.y, forward.z],
        _padding2: 0.0,
        screen_size: [width as f32, height as f32],
        _padding3: [0.0; 2],
    };

    let performance_data = renderer::PerformanceData {
        base_voxel_size: 1.0,
        frame_time: 0.016,
        _padding: [0.0; 2],
    };

    // Create buffers
    let camera_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("Camera Buffer"),
        contents: bytemuck::cast_slice(&[camera_data]),
        usage: BufferUsages::UNIFORM,
    });

    let performance_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("Performance Buffer"),
        contents: bytemuck::cast_slice(&[performance_data]),
        usage: BufferUsages::UNIFORM,
    });

    // Create bind group layouts
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

    // Create bind groups
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

    // Create compute texture
    let compute_texture = device.create_texture(&TextureDescriptor {
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
    let compute_texture_view = compute_texture.create_view(&TextureViewDescriptor::default());

    // Create compute pipeline
    let compute_pipeline = renderer::compute_pipeline::ComputePipeline::new(
        device,
        &camera_bind_group_layout,
        &performance_bind_group_layout,
    );

    // Create blit pipeline
    let blit_pipeline = renderer::blit_pipeline::BlitPipeline::new(device, TextureFormat::Rgba8UnormSrgb);

    // Render frame
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });

    // Run compute shader
    compute_pipeline.dispatch(
        device,
        &mut encoder,
        &compute_texture_view,
        &camera_bind_group,
        &performance_bind_group,
        width,
        height,
    );

    // Blit to target
    blit_pipeline.blit(
        device,
        &mut encoder,
        &compute_texture_view,
        target,
    );

    queue.submit(std::iter::once(encoder.finish()));
}
