let
  rev = "78e723925daf5c9e8d0a1837ec27059e61649cb6";
in
with import <nixpkgs> {
  overlays = [ (import (builtins.fetchTarball "https://github.com/mozilla/nixpkgs-mozilla/archive/${rev}.tar.gz")) ];
};

let
  rust = (rustChannelOf { channel = "1.70.0"; }).rust.override {
    targets = [ "thumbv7m-none-eabi" ];
    extensions = [ "rust-src" ];
  };
in
mkShell {
  buildInputs = [
    cargo-binutils
    fio
    inetutils
    openocd
    rust
    sccache
  ];
  shellHook = "export RUSTC_WRAPPER=sccache";
}
