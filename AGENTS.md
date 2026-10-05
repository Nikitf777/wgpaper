This is a Wayland wallpaper utility with GPU-accelerated transition effects, specified via user-provided shaders.

Project structure (the most important files and directories):
```
.
├── lib-wgpaper-daemon - application's core
│   ├── build.rs
│   └── src
│       ├── app
│       ├── image_wrapper.rs
│       ├── lib.rs
│       ├── renderer
│       │   ├── mod.rs
│       │   └── wgpu
│       ├── transition.rs
│       └── utilities
├── mise.toml
├── README.md
├── wgpaper-config
├── wgpaper-daemon-http - HTTP daemon built with Actix Web
└── wgpaper-shaders - shaders via `rust-gpu`
```

It uses the `smithay-client-toolkit` and `wgpu`.

The project uses the `mise` task runner. Use `mise task` to lists available tasks.
