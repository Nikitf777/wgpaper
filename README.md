# wgpaper*
## Work in progress.
A Wayland (wl-roots) wallpaper utility that supports transition effects with overlapping, like swww, but via user-provided custom shaders.

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
