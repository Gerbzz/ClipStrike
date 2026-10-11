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
    gamma: f32,
    hue: f32,
    chroma_key_threshold: f32,
    chroma_key_fuzziness: f32,
    lut_enabled: f32,
    color_tint: vec3<f32>,
    _pad2: f32,
    chroma_key_color: vec3<f32>,
    _pad3: f32,
    transform: mat4x4<f32>,
}
@group(0) @binding(2) var<uniform> uniforms: FxUniforms;
@group(0) @binding(3) var t_lut: texture_3d<f32>;
@group(0) @binding(4) var s_lut: sampler;

fn luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.bg, K.wz), vec4<f32>(c.gb, K.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));

    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = textureDimensions(t_out);
    let output_coords = vec2<i32>(global_id.xy);
    if (output_coords.x >= i32(dims.x) || output_coords.y >= i32(dims.y)) {
        return;
    }

    // The clip matrix maps source pixels to output pixels. Inverse sampling
    // writes every output pixel, including transparent borders after a move.
    let m = uniforms.transform;
    let det = m[0][0] * m[1][1] - m[1][0] * m[0][1];
    if (abs(det) < 0.000001) {
        textureStore(t_out, output_coords, vec4<f32>(0.0));
        return;
    }
    let p = vec2<f32>(output_coords) - vec2<f32>(m[3][0], m[3][1]);
    let source = vec2<f32>(
        (m[1][1] * p.x - m[1][0] * p.y) / det,
        (m[0][0] * p.y - m[0][1] * p.x) / det
    );
    var coords = vec2<i32>(round(source));
    if (coords.x < 0 || coords.y < 0 || coords.x >= i32(dims.x) || coords.y >= i32(dims.y)) {
        textureStore(t_out, output_coords, vec4<f32>(0.0));
        return;
    }

    // Task 10.13: Pixelate
    if (uniforms.pixelate_size > 1.0) {
        let p_size = i32(uniforms.pixelate_size);
        coords = (coords / p_size) * p_size;
    }

    // Task 10.9: Blur
    var color = vec3<f32>(0.0);
    var source_alpha = 0.0;
    var total_weight = 0.0;
    let radius = i32(uniforms.blur_radius);

    if (radius > 0) {
        for (var i = -radius; i <= radius; i++) {
            for (var j = -radius; j <= radius; j++) {
                let offset_coords = coords + vec2<i32>(i, j);
                if (offset_coords.x >= 0 && offset_coords.x < i32(dims.x) && 
                    offset_coords.y >= 0 && offset_coords.y < i32(dims.y)) {
                    let sample = textureLoad(t_in, offset_coords, 0);
                    color += sample.rgb;
                    source_alpha += sample.a;
                    total_weight += 1.0;
                }
            }
        }
        color /= total_weight;
        source_alpha /= total_weight;
    } else {
        let sample = textureLoad(t_in, coords, 0);
        color = sample.rgb;
        source_alpha = sample.a;
    }

    var final_alpha = source_alpha * uniforms.alpha;

    // Task 10.6: Chroma Key
    if (uniforms.chroma_key_threshold > 0.0) {
        let diff = distance(color, uniforms.chroma_key_color);
        let mask = smoothstep(uniforms.chroma_key_threshold, uniforms.chroma_key_threshold + uniforms.chroma_key_fuzziness, diff);
        final_alpha *= mask;
    }

    // Task 2.3: LUT Application
    if (uniforms.lut_enabled > 0.5) {
        color = textureSampleLevel(t_lut, s_lut, color, 0.0).rgb;
    }

    // Task 10.4: Hue Rotation
    if (abs(uniforms.hue) > 0.001) {
        var hsv = rgb_to_hsv(color);
        hsv.x = fract(hsv.x + uniforms.hue / 360.0);
        color = hsv_to_rgb(hsv);
    }

    // Task 10.3: Gamma
    if (abs(uniforms.gamma - 1.0) > 0.001) {
        color = pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / uniforms.gamma));
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
    textureStore(t_out, output_coords, vec4<f32>(color, final_alpha));
}
