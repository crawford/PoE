let
  rev = "7c1e8b1dd6ed0043fb4ee0b12b815256b0b9de6f";
in
with import <nixpkgs> {
  overlays = [ (import (builtins.fetchTarball "https://github.com/mozilla/nixpkgs-mozilla/archive/${rev}.tar.gz")) ];
};

let
  rust = (rustChannelOf { channel = "1.66.0"; }).rust.override {
    targets = [ "thumbv7m-none-eabi" ];
    extensions = [
      "clippy-preview"
      "rustfmt-preview"
      "rust-analyzer-preview"
      "rust-src"
      "rust-std"
    ];
  };
in
mkShell {
  buildInputs = [
    cargo-binutils
    fio
    rust
    sccache
  ];
  shellHook = "export RUSTC_WRAPPER=sccache";
}
