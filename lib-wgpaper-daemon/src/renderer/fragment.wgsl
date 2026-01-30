@group(0) @binding(0)
var texture_1: texture_2d<f32>;
@group(0) @binding(1)
var texture_2: texture_2d<f32>;
@group(0) @binding(2)
var texture_sampler: sampler;

struct TransitionProgressUniform {
    progress: f32,
    progress_clamped: f32,
    _pad: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> transition_progress: TransitionProgressUniform;

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) uv: vec2<f32>
) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(uv, center);

    let maxRadius = 1.0;
    let borderWidth = 0.03;
    
    let current_radius = transition_progress.progress * (maxRadius + borderWidth);
    
    let inner_edge = current_radius - borderWidth;
    let outer_edge = current_radius;
    
    let blend_factor = smoothstep(inner_edge, outer_edge, dist);
    
    let colorA = textureSample(texture_1, texture_sampler, uv);
    let colorB = textureSample(texture_2, texture_sampler, uv);
    
    return mix(colorB, colorA, blend_factor);
}
