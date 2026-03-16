@group(0) @binding(0) var t_top: texture_2d<f32>;
@group(0) @binding(1) var t_bottom: texture_2d<f32>;
@group(0) @binding(2) var t_out: texture_storage_2d<rgba8unorm, write>;

struct ComposeUniforms {
    blend_mode: u32,
    opacity: f32,
}
@group(0) @binding(3) var<uniform> uniforms: ComposeUniforms;

// Blend Mode Constants
const BLEND_NORMAL: u32 = 0u;
const BLEND_MULTIPLY: u32 = 1u;
const BLEND_SCREEN: u32 = 2u;
const BLEND_OVERLAY: u32 = 3u;
const BLEND_DARKEN: u32 = 4u;
const BLEND_LIGHTEN: u32 = 5u;
const BLEND_COLOR_DODGE: u32 = 6u;
const BLEND_COLOR_BURN: u32 = 7u;
const BLEND_HARD_LIGHT: u32 = 8u;
const BLEND_SOFT_LIGHT: u32 = 9u;
const BLEND_DIFFERENCE: u32 = 10u;
const BLEND_EXCLUSION: u32 = 11u;
const BLEND_ADD: u32 = 16u;
const BLEND_SUBTRACT: u32 = 17u;

fn blend_multiply(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return b * s;
}

fn blend_screen(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return b + s - (b * s);
}

fn blend_overlay_chan(b: f32, s: f32) -> f32 {
    if (b < 0.5) {
        return 2.0 * b * s;
    } else {
        return 1.0 - 2.0 * (1.0 - b) * (1.0 - s);
    }
}

fn blend_overlay(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_overlay_chan(b.r, s.r),
        blend_overlay_chan(b.g, s.g),
        blend_overlay_chan(b.b, s.b)
    );
}

fn blend_color_dodge(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return b / (1.0 - s + 0.00001);
}

fn blend_color_burn(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return 1.0 - (1.0 - b) / (s + 0.00001);
}

fn blend_hard_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return blend_overlay(s, b);
}

fn blend_soft_light_chan(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    } else {
        var d = 0.0;
        if (b <= 0.25) {
            d = ((16.0 * b - 12.0) * b + 4.0) * b;
        } else {
            d = sqrt(b);
        }
        return b + (2.0 * s - 1.0) * (d - b);
    }
}

fn blend_soft_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_soft_light_chan(b.r, s.r),
        blend_soft_light_chan(b.g, s.g),
        blend_soft_light_chan(b.b, s.b)
    );
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dims = textureDimensions(t_out);
    let coords = vec2<i32>(global_id.xy);

    if (coords.x >= i32(dims.x) || coords.y >= i32(dims.y)) {
        return;
    }

    let top = textureLoad(t_top, coords, 0);
    let bottom = textureLoad(t_bottom, coords, 0);

    let src = top.rgb;
    let dst = bottom.rgb;
    var out_rgb = src;

    switch (uniforms.blend_mode) {
        case 1u: { // BLEND_MULTIPLY
            out_rgb = blend_multiply(dst, src);
        }
        case 2u: { // BLEND_SCREEN
            out_rgb = blend_screen(dst, src);
        }
        case 3u: { // BLEND_OVERLAY
            out_rgb = blend_overlay(dst, src);
        }
        case 4u: { // BLEND_DARKEN
            out_rgb = min(dst, src);
        }
        case 5u: { // BLEND_LIGHTEN
            out_rgb = max(dst, src);
        }
        case 6u: { // BLEND_COLOR_DODGE
            out_rgb = blend_color_dodge(dst, src);
        }
        case 7u: { // BLEND_COLOR_BURN
            out_rgb = blend_color_burn(dst, src);
        }
        case 8u: { // BLEND_HARD_LIGHT
            out_rgb = blend_hard_light(dst, src);
        }
        case 9u: { // BLEND_SOFT_LIGHT
            out_rgb = blend_soft_light(dst, src);
        }
        case 10u: { // BLEND_DIFFERENCE
            out_rgb = abs(dst - src);
        }
        case 11u: { // BLEND_EXCLUSION
            out_rgb = dst + src - 2.0 * dst * src;
        }
        case 16u: { // BLEND_ADD
            out_rgb = dst + src;
        }
        case 17u: { // BLEND_SUBTRACT
            out_rgb = dst - src;
        }
        default: { // BLEND_NORMAL
            out_rgb = src;
        }
    }

    // Alpha Compositing (Standard Porter-Duff source-over)
    // out = src_alpha * src_color + (1 - src_alpha) * dst_color
    let alpha = top.a * uniforms.opacity;
    let final_rgb = mix(dst, out_rgb, alpha);
    
    textureStore(t_out, coords, vec4<f32>(final_rgb, 1.0));
}
