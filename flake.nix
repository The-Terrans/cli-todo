{
  description = "Keyboard-driven SQLite todo terminal app";

  # 26.05 supports Intel macOS as well as Linux and Apple Silicon.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          package = pkgs.callPackage ./.nix/package.nix { };
        in
        {
          default = package;
          cli-todo = package;
        });

      apps = forSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/cli-todo";
          meta.description = "Keyboard-driven SQLite todo terminal app";
        };
        cli-todo = self.apps.${system}.default;
      });

      overlays.default = final: _prev: {
        cli-todo = final.callPackage ./.nix/package.nix { };
      };

      checks = forSystems (system: {
        cli-todo = self.packages.${system}.default;
      });
    };
}
