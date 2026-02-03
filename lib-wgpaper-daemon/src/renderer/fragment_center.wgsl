@group(0) @binding(0)
var texture: texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct PerFrameDataUniforms {
    screen_size: vec2<f32>,
    texture_size: vec2<f32>,
    screen_aspect: f32,
    texture_aspect: f32,
    progress_bezier: f32,
    progress_linear: f32,
};
@group(1) @binding(0)
var<uniform> per_frame: PerFrameDataUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = in.tex_coords;
    // Maps screen pixels directly to texture pixels, centered.
    let scale_vec = per_frame.screen_size / per_frame.texture_size;
    let offset_vec = (per_frame.screen_size - per_frame.texture_size) * 0.5 / per_frame.texture_size;
    uv = uv * scale_vec - offset_vec;
    return textureSample(texture, texture_sampler, uv);
}
