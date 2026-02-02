@group(0) @binding(0)
var texture: texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct ScalingDataUniforms {
    screen_size: vec2<f32>,
    texture_size: vec2<f32>,
};
@group(1) @binding(0)
var<uniform> scaling_data: ScalingDataUniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let screen_aspect = scaling_data.screen_size.x / scaling_data.screen_size.y;
    let tex_aspect = scaling_data.texture_size.x / scaling_data.texture_size.y;
    var uv = in.tex_coords;
    // Maps screen pixels directly to texture pixels, centered.
    let scale_vec = scaling_data.screen_size / scaling_data.texture_size;
    let offset_vec = (scaling_data.screen_size - scaling_data.texture_size) * 0.5 / scaling_data.texture_size;
    uv = uv * scale_vec - offset_vec;
    return textureSample(texture, texture_sampler, uv);
}
