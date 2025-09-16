struct CameraData {
    view_proj: mat4x4<f32>,
    position: vec3<f32>,
    forward: vec3<f32>,
    screen_size: vec2<f32>,
}

struct PerformanceData {
    base_voxel_size: f32,
    frame_time: f32,
}

// Use rgba8unorm for compatibility - runtime will use appropriate format
@group(0) @binding(0) var output_texture: texture_storage_2d<rgba8unorm, write>;
@group(1) @binding(0) var<uniform> camera_data: CameraData;
@group(2) @binding(0) var<uniform> performance_data: PerformanceData;
@group(3) @binding(0) var octree_texture: texture_3d<f32>;
@group(3) @binding(1) var octree_sampler: sampler;

fn get_ray_direction(screen_uv: vec2<f32>, camera: CameraData) -> vec3<f32> {
    // Convert to NDC, but flip Y to correct for inverted image
    let ndc = vec2<f32>(screen_uv.x * 2.0 - 1.0, 1.0 - screen_uv.y * 2.0);
    let aspect_ratio = camera.screen_size.x / camera.screen_size.y;
    let fov_tan = tan(radians(60.0) * 0.5);  // Wider FOV to see more of the room

    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), camera.forward));
    let up = cross(camera.forward, right);

    let ray_dir = normalize(
        camera.forward +
        right * ndc.x * fov_tan * aspect_ratio +
        up * ndc.y * fov_tan
    );

    return ray_dir;
}

fn get_adaptive_step_size(distance_from_camera: f32, base_voxel_size: f32) -> f32 {
    // Adaptive step size based on performance feedback
    // base_voxel_size is adjusted by the performance controller (0.005 to 0.05)
    // Apply distance-based scaling on top of performance-based sizing
    let distance_factor = 1.0 + distance_from_camera * 0.1; // Increase step size with distance

    // Minimum step to not miss walls (wall thickness is 0.05)
    let min_step = 0.005;
    let max_step = 0.05;

    return clamp(base_voxel_size * distance_factor, min_step, max_step);
}

fn ray_box_intersection(ray_origin: vec3<f32>, ray_dir: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>) -> vec2<f32> {
    let inv_dir = 1.0 / ray_dir;
    let t_min = (box_min - ray_origin) * inv_dir;
    let t_max = (box_max - ray_origin) * inv_dir;

    let t1 = min(t_min, t_max);
    let t2 = max(t_min, t_max);

    let t_near = max(max(t1.x, t1.y), t1.z);
    let t_far = min(min(t2.x, t2.y), t2.z);

    if t_far < t_near || t_far < 0.0 {
        return vec2<f32>(-1.0, -1.0);
    }

    return vec2<f32>(max(t_near, 0.0), t_far);
}

fn sample_voxel_from_octree(position: vec3<f32>) -> vec4<f32> {
    // Convert world position to texture coordinates
    // Octree covers -2 to 2 in all dimensions, texture is 0 to 1
    let texture_coords = (position + vec3<f32>(2.0, 2.0, 2.0)) / 4.0;

    // Sample the 3D texture
    return textureSampleLevel(octree_texture, octree_sampler, texture_coords, 0.0);
}

fn volume_scatter(accumulated_color: vec4<f32>, voxel_data: vec4<f32>, step_size: f32) -> vec4<f32> {
    if voxel_data.a < 0.01 {
        return accumulated_color;
    }

    let density = voxel_data.a * step_size;
    let transmission = exp(-density);
    let absorption = 1.0 - transmission;

    let new_color = accumulated_color.rgb * transmission + voxel_data.rgb * absorption;
    let new_alpha = accumulated_color.a + (1.0 - accumulated_color.a) * absorption;

    return vec4<f32>(new_color, new_alpha);
}

@compute @workgroup_size(8, 8, 1)
fn ray_march_compute(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel_coord = vec2<i32>(global_id.xy);
    let screen_size = vec2<f32>(camera_data.screen_size);

    if f32(pixel_coord.x) >= screen_size.x || f32(pixel_coord.y) >= screen_size.y {
        return;
    }

    let screen_uv = (vec2<f32>(global_id.xy) + 0.5) / screen_size;

    let ray_origin = camera_data.position;
    let ray_direction = get_ray_direction(screen_uv, camera_data);

    // Test ray-box intersection with scene bounds (Cornell Box)
    // Cornell Box actual bounds: X: -1 to 1, Y: 0 to 2, Z: 0 to 2
    // Extend slightly to ensure we capture walls
    let scene_min = vec3<f32>(-1.1, -0.1, -0.1);
    let scene_max = vec3<f32>(1.1, 2.1, 2.1);
    let intersection = ray_box_intersection(ray_origin, ray_direction, scene_min, scene_max);

    var accumulated_color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if intersection.x >= 0.0 {
        var current_pos = ray_origin + ray_direction * intersection.x;
        let max_distance = intersection.y - intersection.x;
        var distance_traveled = 0.0;
        var hit_something = false;

        for (var i = 0; i < 500 && distance_traveled < max_distance; i++) {
            let distance_from_camera = length(current_pos - ray_origin);
            let step_size = get_adaptive_step_size(distance_from_camera, performance_data.base_voxel_size);

            let voxel_data = sample_voxel_from_octree(current_pos);

            // If we hit a solid voxel, use its color directly
            if voxel_data.a > 0.5 {
                accumulated_color = voxel_data;
                hit_something = true;
                break;
            }

            current_pos = current_pos + ray_direction * step_size;
            distance_traveled = distance_traveled + step_size;
        }

        // If ray didn't hit anything inside the box, show black (empty space)
        if !hit_something {
            accumulated_color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    } else {
        // Ray missed the scene bounds entirely - show background
        let background = mix(
            vec3<f32>(0.5, 0.7, 0.9),
            vec3<f32>(0.1, 0.2, 0.4),
            screen_uv.y
        );
        accumulated_color = vec4<f32>(background, 1.0);
    }

    textureStore(output_texture, pixel_coord, accumulated_color);
}