@group(0) @binding(0) var t_out: texture_storage_2d<rgba8unorm, write>;

struct GeneratorUniforms {
    generator_type: u32,
    color1: vec4<f32>,
    color2: vec4<f32>,
    width: u32,
    height: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(1) var<uniform> uniforms: GeneratorUniforms;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let coords = vec2<i32>(global_id.xy);
    if (global_id.x >= uniforms.width || global_id.y >= uniforms.height) {
        return;
    }

    let norm_coords = vec2<f32>(global_id.xy) / vec2<f32>(f32(uniforms.width), f32(uniforms.height));
    var color = vec4<f32>(0.0);

    if (uniforms.generator_type == 0u) { // SolidColor
        color = uniforms.color1;
    } else if (uniforms.generator_type == 1u) { // LinearGradient
        color = mix(uniforms.color1, uniforms.color2, norm_coords.x);
    } else if (uniforms.generator_type == 2u) { // RadialGradient
        let dist = distance(norm_coords, vec2<f32>(0.5, 0.5));
        color = mix(uniforms.color1, uniforms.color2, clamp(dist * 2.0, 0.0, 1.0));
    } else if (uniforms.generator_type == 3u) { // TestPattern
        let grid = (global_id.x / 32u + global_id.y / 32u) % 2u;
        if (grid == 0u) {
            color = vec4<f32>(0.5, 0.5, 0.5, 1.0);
        } else {
            color = vec4<f32>(0.2, 0.2, 0.2, 1.0);
        }
    }

    textureStore(t_out, coords, color);
}
