@group(0) @binding(0)
var texture: texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct PerFrameDataUniform {
    global_screen_size: vec2<f32>,
    screen_size: vec2<f32>,
    texture_size: vec2<f32>,
    global_screen_aspect: f32,
    screen_aspect: f32,
    texture_aspect: f32,
    progress_bezier: f32,
    progress_linear: f32,
    bg_color: vec4<f32>
};
@group(1) @binding(0)
var<uniform> per_frame: PerFrameDataUniform;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = in.tex_coords;
    if (per_frame.texture_aspect > per_frame.screen_aspect) {
        // Width is limiting factor -> stretch vertically
        let ratio = per_frame.texture_aspect / per_frame.screen_aspect; // > 1.0
        uv = (uv - 0.5) * vec2<f32>(1.0, ratio) + 0.5;
    } else {
        // Height is limiting factor -> stretch horizontally
        let ratio = per_frame.screen_aspect / per_frame.texture_aspect; // > 1.0
        uv = (uv - 0.5) * vec2<f32>(ratio, 1.0) + 0.5;
    }
    return textureSample(texture, texture_sampler, uv);
}
