use nalgebra as na;
use wgpu::*;

pub mod static_provider;

/// Represents voxel data returned from the octree
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VoxelData {
    pub color: [f32; 3],
    pub density: f32,
    pub emission: [f32; 3],
    pub material_type: u32,  // 0 = diffuse, 1 = metallic, 2 = glass, 3 = emissive
}

impl VoxelData {
    pub fn empty() -> Self {
        Self {
            color: [0.0, 0.0, 0.0],
            density: 0.0,
            emission: [0.0, 0.0, 0.0],
            material_type: 0,
        }
    }

    pub fn solid(color: [f32; 3]) -> Self {
        Self {
            color,
            density: 1.0,
            emission: [0.0, 0.0, 0.0],
            material_type: 0,
        }
    }

    pub fn emissive(color: [f32; 3], emission: [f32; 3]) -> Self {
        Self {
            color,
            density: 1.0,
            emission,
            material_type: 3,
        }
    }
}

/// Core trait for octree implementations
pub trait OctreeProvider: Send + Sync {
    /// Sample voxel data at world position with distance-based LoD
    fn sample_voxel(&self, position: na::Vector3<f32>, distance_from_camera: f32) -> VoxelData;

    /// Get optimal step size for ray marching at this distance
    fn get_step_size(&self, distance_from_camera: f32, base_step_size: f32) -> f32 {
        // Default implementation: distance-based scaling
        let distance_factor = 1.0 + distance_from_camera * 0.1;
        let min_step = 0.005;
        let max_step = 0.05;
        (base_step_size * distance_factor).clamp(min_step, max_step)
    }

    /// Called by performance controller to adjust base resolution
    fn set_performance_target(&mut self, target_voxel_size: f32);

    /// Get the bounding box of the scene
    fn get_bounds(&self) -> (na::Vector3<f32>, na::Vector3<f32>);

    /// Check if provider supports dynamic updates
    fn is_dynamic(&self) -> bool {
        false
    }

    /// Update voxel data (for dynamic providers)
    fn update_voxel(&mut self, _position: na::Vector3<f32>, _data: VoxelData) -> Result<(), String> {
        Err("This provider does not support dynamic updates".to_string())
    }

    /// Bind GPU resources for this provider
    fn bind_gpu_resources(&self, device: &Device) -> (BindGroupLayout, BindGroup);

    /// Update GPU resources if needed (called each frame)
    fn update_gpu_resources(&mut self, _queue: &Queue) {
        // Default: no updates needed
    }
}

/// Octree node structure for spatial subdivision
#[derive(Clone, Debug)]
pub struct OctreeNode {
    pub center: na::Vector3<f32>,
    pub half_size: f32,
    pub children: Option<Box<[OctreeNode; 8]>>,
    pub voxel_data: Option<VoxelData>,
    pub level: u8,
}

impl OctreeNode {
    pub fn new(center: na::Vector3<f32>, half_size: f32, level: u8) -> Self {
        Self {
            center,
            half_size,
            children: None,
            voxel_data: None,
            level,
        }
    }

    /// Subdivide this node into 8 children
    pub fn subdivide(&mut self) {
        if self.children.is_some() {
            return;
        }

        let new_half_size = self.half_size * 0.5;
        let new_level = self.level + 1;

        let mut children = Vec::with_capacity(8);
        for i in 0..8 {
            let offset = na::Vector3::new(
                if i & 1 != 0 { new_half_size } else { -new_half_size },
                if i & 2 != 0 { new_half_size } else { -new_half_size },
                if i & 4 != 0 { new_half_size } else { -new_half_size },
            );
            children.push(OctreeNode::new(
                self.center + offset,
                new_half_size,
                new_level,
            ));
        }

        self.children = Some(Box::new(children.try_into().unwrap()));
    }

    /// Get the child index for a given position
    pub fn get_child_index(&self, position: &na::Vector3<f32>) -> usize {
        let mut index = 0;
        if position.x > self.center.x { index |= 1; }
        if position.y > self.center.y { index |= 2; }
        if position.z > self.center.z { index |= 4; }
        index
    }

    /// Check if a position is within this node's bounds
    pub fn contains(&self, position: &na::Vector3<f32>) -> bool {
        (position.x - self.center.x).abs() <= self.half_size &&
        (position.y - self.center.y).abs() <= self.half_size &&
        (position.z - self.center.z).abs() <= self.half_size
    }
}

/// Basic octree structure
pub struct Octree {
    pub root: OctreeNode,
    pub max_depth: u8,
    pub base_voxel_size: f32,
}

impl Octree {
    pub fn new(center: na::Vector3<f32>, half_size: f32, max_depth: u8) -> Self {
        Self {
            root: OctreeNode::new(center, half_size, 0),
            max_depth,
            base_voxel_size: half_size * 2.0 / (1 << max_depth) as f32,
        }
    }

    /// Insert voxel data at a specific position
    pub fn insert(&mut self, position: na::Vector3<f32>, data: VoxelData) {
        Self::insert_recursive(&mut self.root, position, data, 0, self.max_depth);
    }

    fn insert_recursive(node: &mut OctreeNode, position: na::Vector3<f32>, data: VoxelData, depth: u8, max_depth: u8) {
        if !node.contains(&position) {
            return;
        }

        if depth >= max_depth {
            node.voxel_data = Some(data);
            return;
        }

        if node.children.is_none() {
            node.subdivide();
        }

        if let Some(ref mut children) = node.children {
            let mut index = 0;
            if position.x > node.center.x { index |= 1; }
            if position.y > node.center.y { index |= 2; }
            if position.z > node.center.z { index |= 4; }
            Self::insert_recursive(&mut children[index], position, data, depth + 1, max_depth);
        }
    }

    /// Sample voxel data at a position with optional LoD
    pub fn sample(&self, position: na::Vector3<f32>, min_level: u8) -> VoxelData {
        self.sample_recursive(&self.root, position, 0, min_level)
    }

    fn sample_recursive(&self, node: &OctreeNode, position: na::Vector3<f32>, depth: u8, min_level: u8) -> VoxelData {
        if !node.contains(&position) {
            return VoxelData::empty();
        }

        // Use this node's data if we've reached the minimum level or max depth
        if depth >= min_level.min(self.max_depth) {
            if let Some(data) = node.voxel_data {
                return data;
            }
        }

        // Recurse into children if they exist
        if let Some(ref children) = node.children {
            let child_index = node.get_child_index(&position);
            return self.sample_recursive(&children[child_index], position, depth + 1, min_level);
        }

        // Return node's data or empty if no data
        node.voxel_data.unwrap_or_else(VoxelData::empty)
    }
}