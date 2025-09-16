use wgpu::*;
use log::info;

pub struct ComputePipeline {
    pipeline: wgpu::ComputePipeline,
    output_bind_group_layout: BindGroupLayout,
}

impl ComputePipeline {
    pub fn new(
        device: &Device,
        camera_bind_group_layout: &BindGroupLayout,
        performance_bind_group_layout: &BindGroupLayout,
        octree_bind_group_layout: &BindGroupLayout,
    ) -> Self {
        Self::new_with_format(
            device,
            camera_bind_group_layout,
            performance_bind_group_layout,
            octree_bind_group_layout,
            TextureFormat::Rgba8Unorm,
        )
    }

    pub fn new_with_format(
        device: &Device,
        camera_bind_group_layout: &BindGroupLayout,
        performance_bind_group_layout: &BindGroupLayout,
        octree_bind_group_layout: &BindGroupLayout,
        output_format: TextureFormat,
    ) -> Self {
        info!("Creating compute pipeline with format {:?}", output_format);

        let shader_code = include_str!("../shaders/ray_march.wgsl");
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ray March Shader"),
            source: ShaderSource::Wgsl(shader_code.into()),
        });

        let output_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Output Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: output_format,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Compute Pipeline Layout"),
            bind_group_layouts: &[
                &output_bind_group_layout,
                camera_bind_group_layout,
                performance_bind_group_layout,
                octree_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("Ray March Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("ray_march_compute"),
            compilation_options: PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            pipeline,
            output_bind_group_layout,
        }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        output_texture: &TextureView,
        camera_bind_group: &BindGroup,
        performance_bind_group: &BindGroup,
        octree_bind_group: &BindGroup,
        width: u32,
        height: u32,
    ) {
        let output_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Output Bind Group"),
            layout: &self.output_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(output_texture),
                },
            ],
        });

        let mut compute_pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("Ray March Compute Pass"),
            timestamp_writes: None,
        });

        compute_pass.set_pipeline(&self.pipeline);
        compute_pass.set_bind_group(0, &output_bind_group, &[]);
        compute_pass.set_bind_group(1, camera_bind_group, &[]);
        compute_pass.set_bind_group(2, performance_bind_group, &[]);
        compute_pass.set_bind_group(3, octree_bind_group, &[]);

        let workgroup_size = 8;
        let num_workgroups_x = (width + workgroup_size - 1) / workgroup_size;
        let num_workgroups_y = (height + workgroup_size - 1) / workgroup_size;

        compute_pass.dispatch_workgroups(num_workgroups_x, num_workgroups_y, 1);
    }
}