@group(0) @binding(0) var t_top: texture_2d<f32>;
@group(0) @binding(1) var t_bottom: texture_2d<f32>;
@group(0) @binding(2) var t_out: texture_storage_2d<rgba8unorm, write>;

struct ComposeUniforms {
    blend_mode: u32,
    opacity: f32,
    watermark_enabled: u32,
    watermark_opacity: f32,
    watermark_rect: vec4<f32>,
}
@group(0) @binding(3) var<uniform> uniforms: ComposeUniforms;
@group(0) @binding(4) var t_watermark: texture_2d<f32>;
@group(0) @binding(5) var s_watermark: sampler;

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
const BLEND_HUE: u32 = 12u;
const BLEND_SATURATION: u32 = 13u;
const BLEND_COLOR: u32 = 14u;
const BLEND_LUMINOSITY: u32 = 15u;
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

fn rgb_to_hsl(rgb: vec3<f32>) -> vec3<f32> {
    let max_c = max(max(rgb.r, rgb.g), rgb.b);
    let min_c = min(min(rgb.r, rgb.g), rgb.b);
    let l = (max_c + min_c) * 0.5;

    if (max_c == min_c) {
        return vec3<f32>(0.0, 0.0, l);
    }

    let d = max_c - min_c;
    let s = d / (1.0 - abs(2.0 * l - 1.0));
    var h = 0.0;

    if (max_c == rgb.r) {
        h = (rgb.g - rgb.b) / d;
        if (rgb.g < rgb.b) {
            h = h + 6.0;
        }
    } else if (max_c == rgb.g) {
        h = ((rgb.b - rgb.r) / d) + 2.0;
    } else {
        h = ((rgb.r - rgb.g) / d) + 4.0;
    }

    h = h / 6.0;
    return vec3<f32>(h, s, l);
}

fn hue_to_rgb(p: f32, q: f32, t_in: f32) -> f32 {
    var t = t_in;
    if (t < 0.0) {
        t = t + 1.0;
    }
    if (t > 1.0) {
        t = t - 1.0;
    }
    if (t < 1.0 / 6.0) {
        return p + (q - p) * 6.0 * t;
    }
    if (t < 1.0 / 2.0) {
        return q;
    }
    if (t < 2.0 / 3.0) {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    return p;
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let h = hsl.x;
    let s = hsl.y;
    let l = hsl.z;

    if (s <= 0.00001) {
        return vec3<f32>(l, l, l);
    }

    let q = select(l * (1.0 + s), l + s - l * s, l >= 0.5);
    let p = 2.0 * l - q;

    return vec3<f32>(
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0)
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
        case BLEND_MULTIPLY: {
            out_rgb = blend_multiply(dst, src);
        }
        case BLEND_SCREEN: {
            out_rgb = blend_screen(dst, src);
        }
        case BLEND_OVERLAY: {
            out_rgb = blend_overlay(dst, src);
        }
        case BLEND_DARKEN: {
            out_rgb = min(dst, src);
        }
        case BLEND_LIGHTEN: {
            out_rgb = max(dst, src);
        }
        case BLEND_COLOR_DODGE: {
            out_rgb = blend_color_dodge(dst, src);
        }
        case BLEND_COLOR_BURN: {
            out_rgb = blend_color_burn(dst, src);
        }
        case BLEND_HARD_LIGHT: {
            out_rgb = blend_hard_light(dst, src);
        }
        case BLEND_SOFT_LIGHT: {
            out_rgb = blend_soft_light(dst, src);
        }
        case BLEND_DIFFERENCE: {
            out_rgb = abs(dst - src);
        }
        case BLEND_EXCLUSION: {
            out_rgb = dst + src - 2.0 * dst * src;
        }
        case BLEND_HUE: {
            let src_hsl = rgb_to_hsl(src);
            let dst_hsl = rgb_to_hsl(dst);
            out_rgb = hsl_to_rgb(vec3<f32>(src_hsl.x, dst_hsl.y, dst_hsl.z));
        }
        case BLEND_SATURATION: {
            let src_hsl = rgb_to_hsl(src);
            let dst_hsl = rgb_to_hsl(dst);
            out_rgb = hsl_to_rgb(vec3<f32>(dst_hsl.x, src_hsl.y, dst_hsl.z));
        }
        case BLEND_COLOR: {
            let src_hsl = rgb_to_hsl(src);
            let dst_hsl = rgb_to_hsl(dst);
            out_rgb = hsl_to_rgb(vec3<f32>(src_hsl.x, src_hsl.y, dst_hsl.z));
        }
        case BLEND_LUMINOSITY: {
            let src_hsl = rgb_to_hsl(src);
            let dst_hsl = rgb_to_hsl(dst);
            out_rgb = hsl_to_rgb(vec3<f32>(dst_hsl.x, dst_hsl.y, src_hsl.z));
        }
        case BLEND_ADD: {
            out_rgb = dst + src;
        }
        case BLEND_SUBTRACT: {
            out_rgb = dst - src;
        }
        default: {
            out_rgb = src;
        }
    }

    out_rgb = clamp(out_rgb, vec3<f32>(0.0), vec3<f32>(1.0));

    // Alpha Compositing (Standard Porter-Duff source-over)
    // out = src_alpha * src_color + (1 - src_alpha) * dst_color
    let alpha = top.a * uniforms.opacity;
    let final_rgb = mix(dst, out_rgb, alpha);

    var final_color = vec4<f32>(clamp(final_rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);

    if (uniforms.watermark_enabled == 1u) {
        let uv = vec2<f32>(coords) / vec2<f32>(dims);
        let rect = uniforms.watermark_rect; // [x, y, width, height]
        
        if (uv.x >= rect.x && uv.x <= rect.x + rect.z && uv.y >= rect.y && uv.y <= rect.y + rect.w) {
            let watermark_uv = (uv - rect.xy) / rect.zw;
            let watermark_color = textureSampleLevel(t_watermark, s_watermark, watermark_uv, 0.0);
            let w_alpha = watermark_color.a * uniforms.watermark_opacity;
            final_color = vec4<f32>(mix(final_color.rgb, watermark_color.rgb, w_alpha), 1.0);
        }
    }

    textureStore(t_out, coords, final_color);
}
