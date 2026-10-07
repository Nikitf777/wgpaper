This is a Wayland wallpaper utility with GPU-accelerated transition effects, specified via user-provided shaders.
The current name of this project is likely not final (I don't like it).

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

Refer to the roadmap in README.md when making architectural decisions.

It uses the `smithay-client-toolkit` and `wgpu`.

The project uses the `mise` task runner. Use `mise run <task>` to run a task. The main ones are:
- `build`
- `clippy`
- `fmt`
- `run-daemon`
- `test` (no tests yet, need to add some)
To test a wl-roots compositor and multi-monitor setups use the `-sway <output_count(default=1)>` tasks, like this one:
- `run-daemon-sway`
To run an arbitrary commands against a running nested Sway session, use `mise sway-exec <command>`.
Sway was chosen because it is very lightweight, supports the layer shell protocol, allows to launch with multiple outputs and manage them at runtime.
There are also tasks to run commands in the background using `systemd-run`:
- `run-systemd-daemon[-sway]`
- `systemd-daemon[-sway]-logs`
- `status-systemd-daemon[-sway]`
- `stop-systemd-daemon[-sway]`
To run an arbitrary command via `systemd-run`, use `mise run-systemd <unit_name> <command>`.
To reset a unit, use `mise reset-failed-systemd <unit_name>`.
Use `mise task` to lists all the available tasks and `mise task info <task>` to see its arguments and other details.

The project mostly relies on the Jujutsu VCS, so Git is likely to be in a detached state.
