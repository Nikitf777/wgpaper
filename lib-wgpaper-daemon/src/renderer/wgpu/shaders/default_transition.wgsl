@group(0) @binding(0)
var prev_texture: texture_2d<f32>;
@group(0) @binding(1)
var target_texture: texture_2d<f32>;
@group(0) @binding(2)
var texture_sampler: sampler;

struct PerFrameDataUniform {
    screen_size: vec2<f32>,
    texture_size: vec2<f32>,
    screen_aspect: f32,
    texture_aspect: f32,
    progress_bezier: f32,
    progress_linear: f32,
    bg_color: vec4<f32>
};
@group(1) @binding(0)
var<uniform> per_frame: PerFrameDataUniform;

@fragment
fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {
    let prev_color = textureSample(prev_texture, texture_sampler, uv);
    let target_color = textureSample(target_texture, texture_sampler, uv);
    
    return mix(prev_color, target_color, per_frame.progress_bezier);
}