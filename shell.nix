let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/archive/ca1659429c950a11a565b5764e37a1b704e17ec8.tar.gz";
  pkgs = import nixpkgs { };
in

pkgs.mkShell {
  # Pin rustc and cargo with clippy: mixed toolchains fail with E0514.
  packages = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy
    pkg-config
    sqlite
  ];
}
