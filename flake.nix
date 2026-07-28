{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }: let
    pkgs = nixpkgs.legacyPackages.x86_64-linux;

    # Libraries needed at runtime (both for linking and dlopen).
    # nix develop makes these available for build-time (pkg-config, NIX_LD_LIBRARY_PATH),
    # but does NOT add them to LD_LIBRARY_PATH. We need LD_LIBRARY_PATH for programs
    # that use libloading / dlopen to load Wayland, X11, and Vulkan at runtime.
    runtimeLibs = with pkgs; [
      wayland
      libxkbcommon
      libx11
      libxcb
      libXcursor
      libXrandr
      libXi
      vulkan-loader
      libglvnd
    ];
  in {
    devShells.x86_64-linux.default = pkgs.mkShell {
      buildInputs = with pkgs; [
        rustc cargo rustfmt clippy rust-analyzer
        pkg-config gcc
      ] ++ runtimeLibs;

      shellHook = ''
        # Concatenate all runtime library paths into LD_LIBRARY_PATH.
        # makeLibraryPath produces a colon-separated list of lib/ directories
        # from the given packages.
        RUNTIME_LIBS="${pkgs.lib.makeLibraryPath runtimeLibs}"

        # Also add the NixOS GPU driver path (Mesa/RADV and NVIDIA drivers).
        # /run/opengl-driver is a symlink managed by the NixOS system config.
        DRIVER_PATH="/run/opengl-driver/lib"

        export LD_LIBRARY_PATH="$RUNTIME_LIBS:$DRIVER_PATH''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

        # Add GPU driver share path so the Vulkan loader discovers ICDs
        # (nvidia_icd.json, radeon_icd.x86_64.json, etc.) automatically.
        export XDG_DATA_DIRS="/run/opengl-driver/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
      '';
    };
  };
}