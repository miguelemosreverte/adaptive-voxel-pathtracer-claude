use wgpu::*;
use wgpu::util::DeviceExt;
use winit::{
    event::{Event, WindowEvent, ElementState, DeviceEvent},
    event_loop::EventLoop,
    window::Window,
    keyboard::{KeyCode, PhysicalKey},
};
use std::collections::HashSet;
use nalgebra as na;
use std::time::Instant;
use env_logger;
use log::info;
use std::sync::Arc;
use clap::Parser;
use chrono::Local;

mod renderer;
use renderer::VoxelRenderer;
mod benchmark;

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

    /// Duration to run before taking screenshot (seconds)
    #[arg(long, default_value_t = 0.0)]
    duration: f32,

    /// Run performance benchmark
    #[arg(long)]
    benchmark: bool,

    /// Target FPS for adaptive quality system
    #[arg(long, default_value_t = 60.0)]
    target_fps: f32,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    pollster::block_on(run(args));
}

async fn run(args: Args) {
    info!("Starting Adaptive Voxel Path Tracer");

    if args.benchmark {
        benchmark::run_performance_benchmark(args.target_fps).await;
    } else if args.screenshot {
        info!("Screenshot mode enabled");
        run_screenshot_mode(args).await;
    } else {
        run_interactive_mode(args).await;
    }
}

struct Application {
    renderer: VoxelRenderer,
    device: Device,
    queue: Queue,
    camera_position: na::Point3<f32>,
    camera_yaw: f32,   // Horizontal rotation (radians)
    camera_pitch: f32, // Vertical rotation (radians)
    camera_speed: f32,
    mouse_sensitivity: f32,
    keys_pressed: HashSet<KeyCode>,
    performance_monitor: renderer::performance_monitor::PerformanceMonitor,
    last_frame_time: Instant,
}

impl Application {
    fn new(device: Device, queue: Queue, renderer: VoxelRenderer) -> Self {
        Self {
            renderer,
            device,
            queue,
            camera_position: na::Point3::new(0.0, 1.0, -1.0),  // Start outside, looking in
            camera_yaw: 0.0,    // Looking straight ahead (positive Z)
            camera_pitch: 0.0,  // Level view
            camera_speed: 0.05,
            mouse_sensitivity: 0.002,
            keys_pressed: HashSet::new(),
            performance_monitor: renderer::performance_monitor::PerformanceMonitor::new(),
            last_frame_time: Instant::now(),
        }
    }

    fn get_camera_direction(&self) -> na::Vector3<f32> {
        // Calculate forward direction from yaw and pitch
        na::Vector3::new(
            self.camera_yaw.sin() * self.camera_pitch.cos(),
            self.camera_pitch.sin(),
            self.camera_yaw.cos() * self.camera_pitch.cos(),
        )
    }

    fn update_camera(&mut self) {
        let forward = self.get_camera_direction();
        let right = na::Vector3::y().cross(&forward).normalize();

        let mut movement = na::Vector3::zeros();

        // WASD movement (relative to view direction)
        if self.keys_pressed.contains(&KeyCode::KeyW) {
            let forward_horizontal = na::Vector3::new(forward.x, 0.0, forward.z).normalize();
            movement += forward_horizontal; // Forward (no vertical)
        }
        if self.keys_pressed.contains(&KeyCode::KeyS) {
            let forward_horizontal = na::Vector3::new(forward.x, 0.0, forward.z).normalize();
            movement -= forward_horizontal; // Backward
        }
        if self.keys_pressed.contains(&KeyCode::KeyA) {
            movement -= right;
        }
        if self.keys_pressed.contains(&KeyCode::KeyD) {
            movement += right;
        }

        // Vertical movement
        if self.keys_pressed.contains(&KeyCode::Space) {
            movement.y += 1.0;
        }
        if self.keys_pressed.contains(&KeyCode::ShiftLeft) {
            movement.y -= 1.0;
        }

        // Apply movement
        if movement.magnitude() > 0.0 {
            movement = movement.normalize() * self.camera_speed;
            self.camera_position += movement;
        }

        // Always update camera to apply look direction
        let camera_target = self.camera_position + self.get_camera_direction();
        self.renderer.update_camera(&self.queue, self.camera_position, camera_target);
    }

    fn handle_mouse_motion(&mut self, delta_x: f64, delta_y: f64) {
        // Update yaw (horizontal rotation) - positive delta_x should turn right
        self.camera_yaw += delta_x as f32 * self.mouse_sensitivity;

        // Update pitch (vertical rotation) with clamping - positive delta_y should look down
        // But mouse delta_y is inverted (positive = move down), so we need to negate it
        self.camera_pitch -= delta_y as f32 * self.mouse_sensitivity;
        self.camera_pitch = self.camera_pitch.clamp(-1.5, 1.5); // Limit to ~85 degrees up/down
    }

    fn handle_key(&mut self, keycode: KeyCode, state: ElementState) {
        match state {
            ElementState::Pressed => {
                self.keys_pressed.insert(keycode);
            }
            ElementState::Released => {
                self.keys_pressed.remove(&keycode);
            }
        }
    }

    fn render(&mut self) {
        // Track frame time
        let now = Instant::now();
        let frame_time = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;

        // Record performance data
        self.performance_monitor.record_frame(
            frame_time,
            Some([self.camera_position.x, self.camera_position.y, self.camera_position.z])
        );

        // Show FPS in console periodically
        if self.performance_monitor.total_frames % 60 == 0 {
            info!("FPS: {:.1} | Camera: ({:.2}, {:.2}, {:.2})",
                  self.performance_monitor.get_current_fps(),
                  self.camera_position.x,
                  self.camera_position.y,
                  self.camera_position.z);
        }

        self.renderer.render(&self.device, &self.queue);
    }

    fn save_performance_report(&self) {
        let filename = format!("performance_report_{}.md",
                              chrono::Local::now().format("%Y%m%d_%H%M%S"));
        match self.performance_monitor.generate_report(&filename) {
            Ok(_) => info!("Performance report saved to: {}", filename),
            Err(e) => log::error!("Failed to save performance report: {}", e),
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(&self.device, &self.queue, width, height);
    }
}

async fn run_interactive_mode(args: Args) {
    info!("Target FPS: {}", args.target_fps);
    let event_loop = EventLoop::new().unwrap();

    #[allow(deprecated)]
    let window = Arc::new(event_loop.create_window(Window::default_attributes()
        .with_title("Adaptive Voxel Path Tracer - WASD: Move, Space/Shift: Up/Down, ESC: Exit")
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
    let renderer = VoxelRenderer::new(&device, &queue, &adapter, surface, size.width, size.height, args.target_fps);
    let mut app = Application::new(device, queue, renderer);

    // Capture mouse cursor for FPS controls
    let _ = window.set_cursor_grab(winit::window::CursorGrabMode::Confined);
    window.set_cursor_visible(false);

    info!("Controls: WASD - Move, Space/Shift - Up/Down, Mouse - Look around, ESC - Exit");

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
                            app.resize(physical_size.width, physical_size.height);
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        app.update_camera();
                        app.render();
                        window.request_redraw();
                    }
                    WindowEvent::KeyboardInput {
                        event: winit::event::KeyEvent {
                            physical_key: PhysicalKey::Code(keycode),
                            state,
                            ..
                        },
                        ..
                    } => {
                        app.handle_key(keycode, state);

                        // Escape key to exit and save report
                        if keycode == KeyCode::Escape && state == ElementState::Pressed {
                            app.save_performance_report();
                            control_flow.exit();
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion { delta },
                ..
            } => {
                app.handle_mouse_motion(delta.0, delta.1);
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}

async fn run_screenshot_mode(args: Args) {
    use renderer::performance_monitor::PerformanceMonitor;
    use std::time::Duration;

    info!("Initializing headless screenshot renderer");

    // If duration is specified, we'll run for that long before taking screenshot
    let _run_duration = if args.duration > 0.0 {
        Some(Duration::from_secs_f32(args.duration))
    } else {
        None
    };

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
    let fov_y = 60.0_f32.to_radians();  // Wider FOV to match interactive mode
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
