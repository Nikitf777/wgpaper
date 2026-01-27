struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) tex_coords: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Full-screen triangle technique (3 vertices cover entire screen)
    let positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),  // bottom-left
        vec2f(3.0, -1.0),   // extends right to cover screen
        vec2f(-1.0, 3.0),   // extends up to cover screen
    );

    // Calculate UVs that match screen coverage
    let pos = positions[vertex_index];
    out.position = vec4f(pos, 0.0, 1.0);
    out.tex_coords = vec2f(
        (pos.x + 1.0) * 0.5,
        1.0 - (pos.y + 1.0) * 0.5  // Flip Y for proper texture orientation
    );

    return out;
}

@group(0) @binding(0)
var texture1: texture_2d<f32>;
@group(0) @binding(1)
var texture2: texture_2d<f32>;
@group(0) @binding(2)
var texture_sampler: sampler;

struct transition_progress_uniform {
    progress: f32,
    progress_clamped: f32,
    _pad: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> transition_progress: transition_progress_uniform;

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
    
    let colorA = textureSample(texture1, texture_sampler, uv);
    let colorB = textureSample(texture2, texture_sampler, uv);
    
    return mix(colorB, colorA, blend_factor);
}
