{
  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
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
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        devShells.default = with pkgs; mkShell {
          buildInputs = [
            openssl
            pkg-config
            eza
            fd
            (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
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
      }
    );
}
