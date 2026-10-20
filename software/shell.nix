let
  import-archive = org-repo: rev: (import (fetchTarball "https://github.com/${org-repo}/archive/${rev}.tar.gz"));
in
with ((import-archive "NixOS/nixpkgs" "f3a73bcf5d1") { # nixpkgs 26.05
  overlays = [ (import-archive "mozilla/nixpkgs-mozilla" "16ab32eeb83") ];
});

let
  rust = (rustChannelOf { channel = "1.88.0"; }).rust.override {
    targets = [ "thumbv7m-none-eabi" ];
    extensions = [ "rust-src" "rust-analyzer-preview" ];
  };
in
mkShell {
  buildInputs = [
    cargo-binutils
    fio
    inetutils
    openocd
    probe-rs-tools
    rust
    sccache
  ];
  shellHook = "export RUSTC_WRAPPER=sccache";
}
