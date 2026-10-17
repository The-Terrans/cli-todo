{ lib, rustPlatform, pkg-config, sqlite, git }:

rustPlatform.buildRustPackage {
  pname = "cli-todo";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../src ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ sqlite ];
  nativeCheckInputs = [ git ];

  meta = {
    description = "Keyboard-driven SQLite todo terminal app";
    homepage = "https://github.com/The-Terrans/cli-todo";
    mainProgram = "cli-todo";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
