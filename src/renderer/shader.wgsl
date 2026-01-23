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
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
}
