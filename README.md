# wgpaper*
## Work in progress.
A Wayland (wl-roots) wallpaper utility that supports transition effects with overlapping, like swww, but via user-provided custom shaders.

## Custom shaders
A transition shader is a WGSL file that exposes a single fragment entry point:

```wgsl
@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    let prev = textureSample(prev_texture, tex_sampler, tex_coords);
    let next = textureSample(target_texture, tex_sampler, tex_coords);
    return mix(prev, next, per_frame.progress_bezier);
}
```

The uniform struct and every binding are prepended to your file automatically, so
there is nothing to declare. They come from `wgpaper-abi`, which owns the single
declaration of the shader ABI and generates the Rust type, the WGSL, and the
compile-time assertions that keep the two in sync.

| Name             | Type       | Description                                    |
|------------------|------------|------------------------------------------------|
| `prev_texture`   | `texture_2d<f32>` | The frame being transitioned away from.  |
| `target_texture` | `texture_2d<f32>` | The frame being transitioned to.        |
| `tex_sampler`    | `sampler`  | The sampler both textures share.                |
| `per_frame`      | `uniform`  | Per-frame state, below.                         |

`per_frame` exposes `screen_size`, `texture_size`, `virtual_screen_size` and their
`*_aspect` companions, `progress_bezier` / `progress_linear` (both `0.0..=1.0`),
and `bg_color`.

## Roadmap
- [x] - transition effects can overlap each other
- [x] - per-output GPU configuration
- [ ] - expose various random seeds to shaders (per-frame, per-transition, per-output)
- [ ] - expose mouse position to shaders
- [ ] - the ability to start a transition effect on each output independently
- [ ] - the ability to specify an image to start transition to in a request
- [ ] - per-output wallpaper confuration
- [ ] - the ability to specify multiple shaders
- [ ] - per-output shaders
- [ ] - single-canvas emulation mode with the same shader ABI as the normal mode (makes a transition effect look like all the outputs are a part of a single canvas rather than multiple separate ones (e.g. one growing circle for all outputs instead of one circle per output))
- [ ] - replace the current Actix Web-based HTTP daemon with a more lightweigt and fast solution (maybe a binary protocol)
- [ ] - keyboard shortcuts
- [ ] - video wallpapers
- [ ] - load compressed images to VRAM and decode on GPU (might be useless, but I want to try)

This might look over-engineered, but this is intentional. I don't want to build another "simple" wallpaper utility. There are plenty of them.

\* name is likely to change in the future.
