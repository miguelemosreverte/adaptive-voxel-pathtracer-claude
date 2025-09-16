# **Adaptive Voxel Path Tracer: Technical Specification**
*Real-time WebGPU implementation with dynamic Level-of-Detail*

**Date**: September 16, 2025  
**Target Platform**: WebGPU + Rust (wgpu crate)  
**Performance Goal**: 20+ FPS on Apple M1 MacBook  
**Reference Repository**: [Zydak/Vulkan-Path-Tracer](https://github.com/Zydak/Vulkan-Path-Tracer)

---

## **🎯 Reference Implementation**

This project is inspired by **Zydak's Vulkan Path Tracer**, which showcases advanced physically-based rendering with volumetric scattering. We're adapting the core algorithms for WebGPU and adding adaptive performance scaling.

### **Clone Reference Repository**
```bash
# Clone the reference implementation to study algorithms
git clone https://github.com/Zydak/Vulkan-Path-Tracer.git reference-vulkan-pathtracer
cd reference-vulkan-pathtracer

# Key files to study:
# - Volumetric scattering implementation
# - BSSRDF with multiple material types  
# - Ray tracing pipeline structure
# - Denoising algorithms
```

### **Key Features from Reference**
- ✅ **Volumetric Scattering**: Ratio tracking with Henyey-Greenstein phase function
- ✅ **BSSRDF Materials**: Diffuse, Metallic, Dielectric, Glass lobes
- ✅ **Physical Accuracy**: Beer's law for realistic light transport
- ✅ **Real-time Denoising**: Post-processing pipeline
- 🔄 **Our Addition**: Adaptive voxel resolution based on performance

---

## **1. Executive Summary**

This document outlines the complete architecture for a real-time voxel path tracer that adapts scene complexity based on hardware performance. Unlike traditional triangle-based renderers, this system uses true volumetric voxel ray marching with octree acceleration structures, automatically scaling resolution to maintain target framerates across diverse hardware configurations.

**Key Innovation**: Performance-driven adaptive voxelization where scene complexity automatically scales from simple cubes on weak hardware to photorealistic detail on powerful GPUs.

---

## **2. Core Architecture Overview**

### **2.1 System Components**
```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Performance   │───▶│  Octree LoD     │───▶│   Ray Marching  │
│   Feedback      │    │   Manager       │    │    Engine       │
│   Controller    │    │                 │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
        ▲                       │                       │
        │                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Frame Time    │    │  Voxel Data     │    │   WebGPU        │
│   Monitor       │    │  Storage        │    │   Compute       │
│                 │    │                 │    │   Pipeline      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### **2.2 Data Flow**
1. **Performance Monitor** tracks frame times
2. **Feedback Controller** adjusts base voxel resolution
3. **Octree LoD Manager** provides distance-based detail scaling
4. **Ray Marching Engine** traverses voxel data using compute shaders
5. **WebGPU Pipeline** handles GPU resource management

---

## **3. Octree Storage Architecture**

### **3.1 Storage Strategy Decision Matrix**

| Approach | Advantages | Disadvantages | Use Case |
|----------|------------|---------------|----------|
| **3D Storage Texture** | Hardware filtering, mipmapping, compact storage | Limited size (typically 2048³), read-only in many cases | **Static scenes, benchmarks** |
| **Storage Buffer** | Large capacity (128MB+), read/write, flexible indexing | No hardware filtering, manual LoD | **Dynamic scenes, games** |
| **Hybrid Approach** | Best of both: texture for reads, buffer for updates | Complex synchronization, memory overhead | **Our recommended solution** |

### **3.2 Recommended Hybrid Architecture**

```rust
pub struct OctreeStorage {
    // Primary storage: 3D texture for fast ray marching
    voxel_texture: Texture3D<RGBA8>,
    
    // Update buffer: Storage buffer for dynamic modifications
    update_buffer: StorageBuffer<VoxelUpdate>,
    
    // Staging: For CPU->GPU transfers
    staging_buffer: Buffer<VoxelData>,
    
    // Metadata: Octree structure information
    octree_metadata: StorageBuffer<OctreeNode>,
}

pub struct VoxelUpdate {
    position: [u32; 3],
    color: [f32; 4],
    density: f32,
    timestamp: u32,
}
```

### **3.3 Update Mechanism**

**For Static Scenes (Benchmarks)**:
- Load octree data once into 3D texture
- Pure read-only operations
- Maximum performance for ray marching

**For Dynamic Scenes (Games)**:
- **Frame N**: Collect updates in staging buffer
- **Frame N+1**: Copy staging → update buffer via compute shader
- **Frame N+2**: Merge updates into main texture
- **3-frame pipeline latency** acceptable for real-time games

---

## **4. Trait-Based Octree System**

### **4.1 Core Trait Definition**

```rust
pub trait OctreeProvider: Send + Sync {
    /// Get voxel data at world position with distance-based LoD
    fn sample_voxel(&self, position: Vec3, distance_from_camera: f32) -> VoxelData;
    
    /// Update voxel data (for dynamic providers)
    fn update_voxel(&mut self, position: Vec3, data: VoxelData) -> Result<(), UpdateError>;
    
    /// Get optimal step size for ray marching at this distance
    fn get_step_size(&self, distance_from_camera: f32) -> f32;
    
    /// Called by performance controller to adjust base resolution
    fn set_performance_target(&mut self, target_voxel_size: f32);
    
    /// GPU resource binding
    fn bind_gpu_resources(&self, bind_group: &mut BindGroup);
}
```

### **4.2 Implementation Variants**

```rust
pub struct StaticOctreeProvider {
    texture_3d: Texture3D<RGBA8>,
    base_voxel_size: f32,
}

pub struct DynamicOctreeProvider {
    octree_storage: OctreeStorage,
    dirty_regions: HashSet<OctreeNodeId>,
    update_queue: VecDeque<VoxelUpdate>,
}

pub struct StreamingOctreeProvider {
    loaded_chunks: HashMap<ChunkId, OctreeChunk>,
    streaming_system: ChunkStreamer,
    lru_cache: LRUCache<ChunkId, OctreeChunk>,
}
```

---

## **5. Adaptive Performance System**

### **5.1 Performance Feedback Loop**

```rust
pub struct PerformanceController {
    target_framerate: f32,        // 20.0 FPS minimum
    current_voxel_size: f32,      // Current world space voxel size
    frame_time_history: VecDeque<f32>,
    adjustment_rate: f32,         // How aggressively to adjust
}

impl PerformanceController {
    pub fn update(&mut self, frame_time: f32) -> Option<f32> {
        self.frame_time_history.push_back(frame_time);
        
        let avg_frame_time = self.average_frame_time();
        let target_frame_time = 1.0 / self.target_framerate;
        
        if avg_frame_time > target_frame_time * 1.1 {
            // Too slow: increase voxel size (reduce quality)
            self.current_voxel_size *= 1.1;
            Some(self.current_voxel_size)
        } else if avg_frame_time < target_frame_time * 0.8 {
            // Fast enough: decrease voxel size (increase quality)
            self.current_voxel_size *= 0.95;
            Some(self.current_voxel_size)
        } else {
            None // No adjustment needed
        }
    }
}
```

### **5.2 Distance-Based LoD Integration**

```glsl
// WGSL Compute Shader for Ray Marching
fn get_adaptive_step_size(distance_from_camera: f32, base_voxel_size: f32) -> f32 {
    // Performance-driven base size
    let performance_multiplier = base_voxel_size;
    
    // Distance-driven scaling
    let distance_multiplier = max(1.0, distance_from_camera / 10.0);
    
    return performance_multiplier * distance_multiplier;
}

@compute @workgroup_size(8, 8, 1)
fn ray_march_compute(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @group(0) @binding(0) var voxel_texture: texture_3d<f32>,
    @group(0) @binding(1) var output_texture: texture_storage_2d<rgba8unorm, write>,
    @group(1) @binding(0) var<uniform> camera_data: CameraData,
    @group(1) @binding(1) var<uniform> performance_data: PerformanceData,
) {
    let pixel_coord = vec2<i32>(global_id.xy);
    let screen_uv = vec2<f32>(global_id.xy) / vec2<f32>(camera_data.screen_size);
    
    // Generate ray
    let ray_origin = camera_data.position;
    let ray_direction = get_ray_direction(screen_uv, camera_data);
    
    // Adaptive ray marching with volumetrics (inspired by Zydak's approach)
    var current_pos = ray_origin;
    var accumulated_color = vec4<f32>(0.0);
    
    for (var i = 0; i < 1000; i++) {
        let distance_from_camera = length(current_pos - ray_origin);
        let step_size = get_adaptive_step_size(distance_from_camera, performance_data.base_voxel_size);
        
        // Sample voxel density and color
        let voxel_data = textureSampleLevel(voxel_texture, sampler, current_pos, 0.0);
        
        // Volumetric scattering (Beer's law + Henyey-Greenstein phase function)
        accumulated_color = volume_scatter(accumulated_color, voxel_data, step_size);
        
        // Early termination if opaque
        if (accumulated_color.a > 0.99) { break; }
        
        current_pos += ray_direction * step_size;
    }
    
    textureStore(output_texture, pixel_coord, accumulated_color);
}
```

---

## **6. Implementation Roadmap**

### **Phase 1: Foundation (Week 1-2)**
```rust
// 1. Basic WebGPU setup with wgpu
cargo new adaptive_voxel_tracer
cd adaptive_voxel_tracer
cargo add wgpu winit bytemuck nalgebra

// 2. Basic window and GPU context
struct VoxelRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    // ... other WebGPU resources
}

// 3. Simple cube voxel test
// - Single 1m³ voxel in world space
// - Basic ray-cube intersection
// - Verify 20+ FPS on M1 MacBook
```

### **Phase 2: Octree Foundation (Week 3-4)**
```rust
// 4. Implement basic octree structure
pub struct Octree {
    root: OctreeNode,
    max_depth: u8,
    base_voxel_size: f32,
}

// 5. 3D DDA ray traversal
fn traverse_octree_dda(
    ray_origin: Vec3,
    ray_direction: Vec3,
    octree: &Octree
) -> Vec<VoxelHit> { /* ... */ }

// 6. Performance monitoring
let mut performance_controller = PerformanceController::new(20.0);
```

### **Phase 3-5: Advanced Features**
- **Phase 3**: Adaptive System & Dynamic Updates (Week 5-6)
- **Phase 4**: Volumetric Materials & Lighting (Week 7-8)  
- **Phase 5**: Optimization & Polish (Week 9-10)

---

## **7. Performance Targets & Scaling**

### **7.1 Hardware Performance Matrix**

| Hardware Class | Base Voxel Size | Max Octree Depth | Expected FPS | Visual Quality |
|----------------|----------------|------------------|--------------|----------------|
| **Weak CPU** | 2.0m³ | 3 levels | 20-30 FPS | Simple blocks |
| **Apple M1** | 0.5m³ | 5 levels | 30-60 FPS | Detailed voxels |
| **Gaming GPU** | 0.1m³ | 8 levels | 60+ FPS | Near-photorealistic |
| **RTX 4090** | 0.02m³ | 10 levels | 120+ FPS | Photorealistic |

### **7.2 Scalability Strategy**

**Automatic Quality Scaling**:
- Monitor frame times continuously
- Adjust voxel size by 5-10% per frame when needed
- Hysteresis to prevent oscillation
- User can set minimum quality floor

**Distance-Based LoD**:
- Objects closer than 5m: Full resolution
- Objects 5-20m away: 2x larger voxels
- Objects 20-100m away: 4x larger voxels  
- Objects beyond 100m: 8x larger voxels

---

## **8. Technical Challenges & Solutions**

### **8.1 Challenge: WebGPU Compute Shader Limitations**
**Problem**: No hardware ray tracing acceleration
**Solution**: Optimized 3D DDA traversal with octree pruning

### **8.2 Challenge: Memory Bandwidth on M1**
**Problem**: Limited memory bandwidth for large voxel datasets
**Solution**: Compressed voxel storage + streaming system

### **8.3 Challenge: Dynamic Scene Updates**
**Problem**: Updating 3D textures is expensive
**Solution**: Hybrid storage with incremental updates

### **8.4 Challenge: Real-time Performance**
**Problem**: Path tracing is inherently expensive
**Solution**: 
- Very low sample counts (1-4 samples per pixel)
- Temporal accumulation when camera is static
- Real-time denoising with edge-preserving filters

---

## **9. Success Metrics**

### **9.1 Performance Metrics**
- ✅ **Primary Goal**: 20+ FPS on Apple M1 MacBook
- ✅ **Scalability**: Automatic adaptation to hardware capabilities
- ✅ **Visual Quality**: Recognizable objects at minimum quality
- ✅ **Responsiveness**: <100ms latency for voxel updates

### **9.2 Technical Metrics**
- ✅ **Memory Usage**: <2GB GPU memory for typical scenes
- ✅ **Loading Times**: <5 seconds for medium complexity scenes
- ✅ **Code Quality**: Trait-based architecture allows easy extensions

---

## **10. Getting Started**

### **10.1 Development Environment Setup**
```bash
# 1. Clone reference repository to study algorithms
git clone https://github.com/Zydak/Vulkan-Path-Tracer.git reference-vulkan-pathtracer

# 2. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 3. Create new project
cargo new adaptive-voxel-pathtracer --bin
cd adaptive-voxel-pathtracer

# 4. Add dependencies
cargo add wgpu winit bytemuck nalgebra pollster
```

### **10.2 Minimal Starting Code Template**
```rust
// src/main.rs - Minimal WebGPU setup
use wgpu::*;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Adaptive Voxel Path Tracer")
        .build(&event_loop).unwrap();
    
    // Initialize WebGPU
    let instance = Instance::new(InstanceDescriptor::default());
    let surface = unsafe { instance.create_surface(&window) }.unwrap();
    let adapter = instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }).await.unwrap();
    
    let (device, queue) = adapter.request_device(
        &DeviceDescriptor::default(),
        None,
    ).await.unwrap();
    
    // TODO: Implement voxel renderer
    let mut renderer = VoxelRenderer::new(device, queue, surface);
    
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { window_id, event } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(physical_size) => {
                        renderer.resize(physical_size);
                    }
                    _ => {}
                }
            }
            Event::RedrawRequested(_) => {
                renderer.render();
            }
            _ => {}
        }
    });
}
```

---

## **11. Future Extensions**

### **11.1 Advanced Features**
- **Volumetric Lighting**: God rays, fog, atmospheric scattering
- **Material System**: Glass, metals, emissive surfaces (from Zydak's BSSRDF)
- **Animation Support**: Moving/rotating objects
- **Physics Integration**: Voxel-based collision detection

### **11.2 Platform Expansion**
- **WASM Optimization**: For web deployment
- **Mobile Support**: iOS/Android with Metal/Vulkan backends
- **VR/AR**: Spatial computing applications

---

## **📚 Study Materials**

### **Key Papers & Resources**
- **Production Volume Rendering 2017**: Volumetric scattering techniques
- **Turquin 2018**: Energy compensation in BSSRDF
- **Eric Heitz 2018**: Anisotropic materials in path tracing
- **WebGPU Fundamentals**: https://webgpufundamentals.org/
- **WGPU Book**: https://sotrh.github.io/learn-wgpu/

### **Code Structure Inspiration**
```
adaptive-voxel-pathtracer/
├── src/
│   ├── main.rs                    # Entry point
│   ├── renderer/
│   │   ├── mod.rs                 # Main renderer
│   │   ├── compute_pipeline.rs    # Ray marching shaders
│   │   └── performance.rs         # Performance controller
│   ├── octree/
│   │   ├── mod.rs                 # Octree trait
│   │   ├── static_provider.rs     # Benchmark version
│   │   └── dynamic_provider.rs    # Game version
│   └── shaders/
│       ├── ray_march.wgsl         # Main compute shader
│       └── volume_scatter.wgsl    # Volumetric lighting
├── reference-vulkan-pathtracer/   # Cloned reference
└── CLAUDE.md                      # This document
```

---

**🎯 Ready to Start Building!**

This specification provides the complete foundation for building an adaptive voxel path tracer that scales from simple cubes on weak hardware to photorealistic rendering on powerful GPUs, all while maintaining real-time performance through intelligent performance feedback and distance-based level of detail.

*Last updated: September 16, 2025*