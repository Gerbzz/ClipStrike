@group(0) @binding(0) var t_in: texture_2d<f32>;
@group(0) @binding(1) var t_out: texture_storage_2d<rgba8unorm, write>;

struct FxUniforms {
    brightness: f32,
    contrast: f32,
    saturation: f32,
    alpha: f32,
    grayscale: f32,
    blur_radius: f32,
    pixelate_size: f32,
    color_tint: vec3<f32>,
    transform: mat4x4<f32>,
}
@group(0) @binding(2) var<uniform> uniforms: FxUniforms;

fn luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = textureDimensions(t_in);
    var coords = vec2<i32>(global_id.xy);

    if (coords.x >= i32(dims.x) || coords.y >= i32(dims.y)) {
        return;
    }

    // Task 10.13: Pixelate
    if (uniforms.pixelate_size > 1.0) {
        let p_size = i32(uniforms.pixelate_size);
        coords = (coords / p_size) * p_size;
    }

    // Task 10.9: Blur (current box blur, optimization pending)
    var color = vec3<f32>(0.0);
    var total_weight = 0.0;
    let radius = i32(uniforms.blur_radius);

    if (radius > 0) {
        for (var i = -radius; i <= radius; i++) {
            for (var j = -radius; j <= radius; j++) {
                let offset_coords = coords + vec2<i32>(i, j);
                if (offset_coords.x >= 0 && offset_coords.x < i32(dims.x) && 
                    offset_coords.y >= 0 && offset_coords.y < i32(dims.y)) {
                    color += textureLoad(t_in, offset_coords, 0).rgb;
                    total_weight += 1.0;
                }
            }
        }
        color /= total_weight;
    } else {
        color = textureLoad(t_in, coords, 0).rgb;
    }

    // Task 10.4: Color Correction (Tint)
    color = color * uniforms.color_tint;

    // Apply brightness
    color = color * uniforms.brightness;

    // Apply contrast
    color = (color - 0.5) * uniforms.contrast + 0.5;

    // Apply saturation
    let l = luma(color);
    color = mix(vec3<f32>(l), color, uniforms.saturation);

    // Task 10.5: Grayscale
    color = mix(color, vec3<f32>(l), uniforms.grayscale);

    // Apply alpha and write out
    textureStore(t_out, coords, vec4<f32>(color, uniforms.alpha));
}
