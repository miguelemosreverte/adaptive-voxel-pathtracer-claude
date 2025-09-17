use super::{Octree, OctreeProvider, VoxelData};
use nalgebra as na;
use wgpu::*;
use log::info;

/// Static octree provider for benchmark scenes (like Cornell Box)
/// This implementation uses a 3D texture for GPU-accelerated sampling
pub struct StaticOctreeProvider {
    octree: Octree,
    base_voxel_size: f32,
    texture_3d: Option<Texture>,
    texture_view: Option<TextureView>,
    sampler: Option<Sampler>,
    bind_group_layout: Option<BindGroupLayout>,
    bind_group: Option<BindGroup>,
    texture_size: u32,
}

impl StaticOctreeProvider {
    pub fn new_cornell_box() -> Self {
        // Create octree centered at origin for simplicity
        // Cornell Box: X: -1 to 1, Y: 0 to 2, Z: 0 to 2
        let center = na::Vector3::new(0.0, 0.0, 0.0);
        let half_size = 2.0;  // Covers -2 to 2 in all dimensions
        let max_depth = 8;  // 256x256x256 resolution at max depth for better quality

        let mut octree = Octree::new(center, half_size, max_depth);

        // Build the Cornell Box scene in the octree
        Self::build_cornell_box_scene(&mut octree);

        Self {
            octree,
            base_voxel_size: 0.02,
            texture_3d: None,
            texture_view: None,
            sampler: None,
            bind_group_layout: None,
            bind_group: None,
            texture_size: 256,  // 256x256x256 3D texture for better quality
        }
    }

    fn build_cornell_box_scene(octree: &mut Octree) {
        let resolution = 0.02;  // Finer resolution for better quality
        let mut voxel_count = 0;
        let mut ceiling_count = 0;
        let mut light_count = 0;

        // Sample the scene and insert into octree
        for x in -60..=60 {
            for y in -10..=110 {  // Extend Y range to ensure we capture ceiling at Y=2
                for z in -10..=110 {
                    let pos = na::Vector3::new(
                        x as f32 * resolution,
                        y as f32 * resolution,
                        z as f32 * resolution,
                    );

                    if let Some(voxel) = Self::sample_cornell_box_at(pos) {
                        octree.insert(pos, voxel);
                        voxel_count += 1;

                        // Count ceiling and light voxels for debugging
                        if pos.y >= 1.95 && pos.y <= 2.05 {
                            ceiling_count += 1;
                            if pos.x >= -0.25 && pos.x <= 0.25 && pos.z >= 0.75 && pos.z <= 1.25 {
                                light_count += 1;
                            }
                        }
                    }
                }
            }
        }

        info!("Built Cornell Box scene: {} total voxels, {} ceiling, {} light",
              voxel_count, ceiling_count, light_count);
    }

    fn sample_cornell_box_at(pos: na::Vector3<f32>) -> Option<VoxelData> {
        let wall_thickness = 0.05;

        // Floor (white)
        if pos.y >= -wall_thickness && pos.y <= wall_thickness {
            if pos.x >= -1.0 && pos.x <= 1.0 && pos.z >= 0.0 && pos.z <= 2.0 {
                return Some(VoxelData::solid([0.73, 0.73, 0.73]));
            }
        }

        // Ceiling (white)
        if pos.y >= 2.0 - wall_thickness && pos.y <= 2.0 + wall_thickness {
            if pos.x >= -1.0 && pos.x <= 1.0 && pos.z >= 0.0 && pos.z <= 2.0 {
                // Light source in center of ceiling
                if pos.x >= -0.25 && pos.x <= 0.25 && pos.z >= 0.75 && pos.z <= 1.25 {
                    return Some(VoxelData::emissive([1.0, 1.0, 0.95], [5.0, 5.0, 4.75]));
                }
                return Some(VoxelData::solid([0.73, 0.73, 0.73]));
            }
        }

        // Back wall (white)
        if pos.z >= 2.0 - wall_thickness && pos.z <= 2.0 + wall_thickness {
            if pos.x >= -1.0 - wall_thickness && pos.x <= 1.0 + wall_thickness &&
               pos.y >= -wall_thickness && pos.y <= 2.0 + wall_thickness {
                return Some(VoxelData::solid([0.73, 0.73, 0.73]));
            }
        }

        // Left wall (red)
        if pos.x >= -1.0 - wall_thickness && pos.x <= -1.0 + wall_thickness {
            if pos.z >= 0.0 && pos.z <= 2.0 && pos.y >= 0.0 && pos.y <= 2.0 {
                return Some(VoxelData::solid([0.65, 0.05, 0.05]));
            }
        }

        // Right wall (green)
        if pos.x >= 1.0 - wall_thickness && pos.x <= 1.0 + wall_thickness {
            if pos.z >= 0.0 && pos.z <= 2.0 && pos.y >= 0.0 && pos.y <= 2.0 {
                return Some(VoxelData::solid([0.12, 0.45, 0.15]));
            }
        }

        // Tall box (white)
        let tall_center = na::Vector3::new(-0.35, 0.3, 0.65);
        let tall_half = na::Vector3::new(0.15, 0.3, 0.15);

        // Simple rotation around Y
        let cos_a = 0.956;
        let sin_a = -0.292;
        let offset = pos - tall_center;
        let rotated_x = offset.x * cos_a - offset.z * sin_a;
        let rotated_z = offset.x * sin_a + offset.z * cos_a;

        if rotated_x.abs() <= tall_half.x &&
           pos.y >= 0.0 && pos.y <= tall_half.y * 2.0 &&
           rotated_z.abs() <= tall_half.z {
            return Some(VoxelData::solid([0.73, 0.73, 0.73]));
        }

        // Short box (white)
        let short_center = na::Vector3::new(0.35, 0.15, 1.35);
        let short_half = na::Vector3::new(0.15, 0.15, 0.15);

        let offset2 = pos - short_center;
        let rotated_x2 = offset2.x * cos_a + offset2.z * sin_a;
        let rotated_z2 = -offset2.x * sin_a + offset2.z * cos_a;

        if rotated_x2.abs() <= short_half.x &&
           pos.y >= 0.0 && pos.y <= short_half.y * 2.0 &&
           rotated_z2.abs() <= short_half.z {
            return Some(VoxelData::solid([0.73, 0.73, 0.73]));
        }

        None
    }

    /// Create 3D texture from octree data
    pub fn create_texture(&mut self, device: &Device, queue: &Queue) {
        let size = self.texture_size;
        let mut texture_data = vec![0u8; (size * size * size * 4) as usize];

        // Sample octree into texture
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let world_pos = na::Vector3::new(
                        (x as f32 / size as f32 - 0.5) * 4.0,  // -2 to 2
                        (y as f32 / size as f32 - 0.5) * 4.0,  // -2 to 2
                        (z as f32 / size as f32 - 0.5) * 4.0,  // -2 to 2
                    );

                    let voxel = self.octree.sample(world_pos, 0);
                    let idx = ((z * size * size + y * size + x) * 4) as usize;

                    // Pack as RGBA8
                    texture_data[idx] = (voxel.color[0] * 255.0) as u8;
                    texture_data[idx + 1] = (voxel.color[1] * 255.0) as u8;
                    texture_data[idx + 2] = (voxel.color[2] * 255.0) as u8;
                    texture_data[idx + 3] = (voxel.density * 255.0) as u8;
                }
            }
        }

        // Create 3D texture
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Octree 3D Texture"),
            size: Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: size,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D3,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Write all texture data at once for the 3D texture
        queue.write_texture(
            texture.as_image_copy(),
            &texture_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size),
                rows_per_image: Some(size),
            }.into(),
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: size,
            },
        );

        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        // Create sampler with linear filtering for smoother appearance
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("Octree Sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,  // Linear for smoother interpolation
            min_filter: FilterMode::Linear,  // Linear for smoother interpolation
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        self.texture_3d = Some(texture);
        self.texture_view = Some(texture_view);
        self.sampler = Some(sampler);

        info!("Created {}x{}x{} 3D texture for octree", size, size, size);
    }
}

impl OctreeProvider for StaticOctreeProvider {
    fn sample_voxel(&self, position: na::Vector3<f32>, distance_from_camera: f32) -> VoxelData {
        // Calculate LoD level based on distance
        let lod_level = (distance_from_camera / 5.0).floor() as u8;
        self.octree.sample(position, lod_level.min(self.octree.max_depth))
    }

    fn set_performance_target(&mut self, target_voxel_size: f32) {
        self.base_voxel_size = target_voxel_size;
    }

    fn get_bounds(&self) -> (na::Vector3<f32>, na::Vector3<f32>) {
        let center = self.octree.root.center;
        let half = self.octree.root.half_size;
        (
            center - na::Vector3::new(half, half, half),
            center + na::Vector3::new(half, half, half),
        )
    }

    fn bind_gpu_resources(&self, device: &Device) -> (BindGroupLayout, BindGroup) {
        // Create bind group layout if not exists
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Octree Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Octree Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(self.texture_view.as_ref().unwrap()),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(self.sampler.as_ref().unwrap()),
                },
            ],
        });

        (bind_group_layout, bind_group)
    }
}