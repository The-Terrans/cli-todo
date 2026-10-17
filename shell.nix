let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/archive/ca1659429c950a11a565b5764e37a1b704e17ec8.tar.gz";
  pkgs = import nixpkgs { };
in

pkgs.mkShell {
  packages = with pkgs; [
    rustfmt
    clippy
    pkg-config
    sqlite
  ];
}
