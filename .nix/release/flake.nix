{
  description = "Prebuilt cli-todo release (no Rust compilation)";
  # 26.05 still supports Intel macOS; unstable has dropped x86_64-darwin.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs, ... }:
    let
      release =
        if builtins.pathExists ./release.json then
          builtins.fromJSON (builtins.readFile ./release.json)
        else
          throw "Use the cli-todo-flake.tar.gz attached to a GitHub release; this file is a release template.";
      systems = builtins.attrNames release.artifacts;
      forSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          artifact = release.artifacts.${system};
          package = pkgs.stdenvNoCC.mkDerivation {
            pname = "cli-todo";
            version = release.version;
            src = pkgs.fetchurl {
              url = "https://github.com/${release.repository}/releases/download/${release.tag}/${artifact.file}";
              hash = artifact.hash;
            };
            sourceRoot = ".";
            dontConfigure = true;
            dontBuild = true;
            dontStrip = true;
            dontPatchELF = true;
            strictDeps = true;
            installPhase = ''
              runHook preInstall
              install -Dm755 cli-todo "$out/bin/cli-todo"
              runHook postInstall
            '';
            meta = {
              description = "Keyboard-driven SQLite todo terminal app";
              homepage = "https://github.com/${release.repository}";
              mainProgram = "cli-todo";
              platforms = [ system ];
              sourceProvenance = [ pkgs.lib.sourceTypes.binaryNativeCode ];
            };
          };
        in
        {
          default = package;
          cli-todo = package;
        }
      );
      apps = forSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/cli-todo";
        };
        cli-todo = self.apps.${system}.default;
      });
    };
}
