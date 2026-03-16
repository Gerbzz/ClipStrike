// The core display shader for ClipStrike.
// Takes a simple texture and renders it to the screen. 
// Later, this will be expanded in Phase 4 to handle compute pipelines (Color Grading, Keys)

@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.uv = model.uv;
    out.clip_position = vec4<f32>(model.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Read the pixel from our hardware-decoded video texture
    let raw_color = textureSample(t_diffuse, s_diffuse, in.uv);
    
    // YUV to RGB math is typically handled by GStreamer glupload before it 
    // hits this shader, but if we need a custom matrix applied later it goes here.
    return raw_color;
}
