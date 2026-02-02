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
    if (tex_aspect > screen_aspect) {
        // Width is limiting factor -> stretch vertically
        let ratio = tex_aspect / screen_aspect; // > 1.0
        uv = (uv - 0.5) * vec2<f32>(1.0, ratio) + 0.5;
    } else {
        // Height is limiting factor -> stretch horizontally
        let ratio = screen_aspect / tex_aspect; // > 1.0
        uv = (uv - 0.5) * vec2<f32>(ratio, 1.0) + 0.5;
    }
    return textureSample(texture, texture_sampler, uv);
}
