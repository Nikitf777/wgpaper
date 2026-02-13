struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) tex_coords: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    let positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(3.0, -1.0),
        vec2f(-1.0, 3.0),
    );

    let pos = positions[vertex_index];
    out.position = vec4f(pos, 0.0, 1.0);
    out.tex_coords = vec2f(
        (pos.x + 1.0) * 0.5,
        1.0 - (pos.y + 1.0) * 0.5
    );

    return out;
}
