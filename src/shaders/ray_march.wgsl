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
    // Use very small steps to ensure we don't miss thin walls
    // Wall thickness is 0.02, so use step size < wall_thickness/2
    return 0.005;
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

fn sample_voxel(position: vec3<f32>) -> vec4<f32> {
    // Cornell Box scene - standard dimensions
    // The box goes from -1 to 1 in X, 0 to 2 in Y, 0 to 2 in Z
    // Camera looks from negative Z towards positive Z (into the box)

    let wall_thickness = 0.05;  // Thicker walls to ensure they're hit

    // Floor (white)
    if position.y >= -wall_thickness && position.y <= wall_thickness {
        if position.x >= -1.0 && position.x <= 1.0 && position.z >= 0.0 && position.z <= 2.0 {
            return vec4<f32>(0.73, 0.73, 0.73, 1.0);
        }
    }

    // Ceiling (white)
    if position.y >= 2.0 - wall_thickness && position.y <= 2.0 + wall_thickness {
        if position.x >= -1.0 && position.x <= 1.0 && position.z >= 0.0 && position.z <= 2.0 {
            // Light source in center of ceiling
            if position.x >= -0.25 && position.x <= 0.25 && position.z >= 0.75 && position.z <= 1.25 {
                return vec4<f32>(1.0, 1.0, 0.95, 1.0); // Light emission
            }
            return vec4<f32>(0.73, 0.73, 0.73, 1.0);
        }
    }

    // Back wall (white) - at far Z
    if position.z >= 2.0 - wall_thickness && position.z <= 2.0 + wall_thickness {
        if position.x >= -1.0 - wall_thickness && position.x <= 1.0 + wall_thickness &&
           position.y >= -wall_thickness && position.y <= 2.0 + wall_thickness {
            return vec4<f32>(0.73, 0.73, 0.73, 1.0);
        }
    }

    // Left wall (red)
    if position.x >= -1.0 - wall_thickness && position.x <= -1.0 + wall_thickness {
        if position.z >= 0.0 && position.z <= 2.0 && position.y >= 0.0 && position.y <= 2.0 {
            return vec4<f32>(0.65, 0.05, 0.05, 1.0);
        }
    }

    // Right wall (green)
    if position.x >= 1.0 - wall_thickness && position.x <= 1.0 + wall_thickness {
        if position.z >= 0.0 && position.z <= 2.0 && position.y >= 0.0 && position.y <= 2.0 {
            return vec4<f32>(0.12, 0.45, 0.15, 1.0);
        }
    }

    // Note: Cornell Box traditionally has no front wall (open where camera looks from)

    // Tall box (white) - on the floor, left side
    let tall_center = vec3<f32>(-0.35, 0.3, 0.65);
    let tall_half_size = vec3<f32>(0.15, 0.3, 0.15);

    // Rotate around Y axis by about 17 degrees
    let cos_a = 0.956;
    let sin_a = -0.292;
    let offset = position - tall_center;
    let rotated_x = offset.x * cos_a - offset.z * sin_a;
    let rotated_z = offset.x * sin_a + offset.z * cos_a;

    if abs(rotated_x) <= tall_half_size.x &&
       position.y >= 0.0 && position.y <= tall_half_size.y * 2.0 &&
       abs(rotated_z) <= tall_half_size.z {
        return vec4<f32>(0.73, 0.73, 0.73, 1.0);
    }

    // Short box (white) - on the floor, right side
    let short_center = vec3<f32>(0.35, 0.15, 1.35);
    let short_half_size = vec3<f32>(0.15, 0.15, 0.15);

    // Rotate around Y axis by about -17 degrees
    let offset2 = position - short_center;
    let rotated_x2 = offset2.x * cos_a + offset2.z * sin_a;
    let rotated_z2 = -offset2.x * sin_a + offset2.z * cos_a;

    if abs(rotated_x2) <= short_half_size.x &&
       position.y >= 0.0 && position.y <= short_half_size.y * 2.0 &&
       abs(rotated_z2) <= short_half_size.z {
        return vec4<f32>(0.73, 0.73, 0.73, 1.0);
    }

    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
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

            let voxel_data = sample_voxel(current_pos);

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