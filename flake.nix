{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }: {
    devShells.x86_64-linux.default = nixpkgs.legacyPackages.x86_64-linux.mkShell {
      buildInputs = with nixpkgs.legacyPackages.x86_64-linux; [
        rustc cargo rustfmt clippy rust-analyzer
        pkg-config gcc
        # Add C libraries your project needs here:
        wayland
        libxkbcommon
      ];
    };
  };
}
