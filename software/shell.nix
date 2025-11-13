let
  nixpkgs = "nixos-22.11";
  nixpkgs-mozilla = "78e723925daf5c9e8d0a1837ec27059e61649cb6";
  import-archive = org-repo: rev: (import (fetchTarball "https://github.com/${org-repo}/archive/${rev}.tar.gz"));
in
with (import-archive "NixOS/nixpkgs" nixpkgs) {
  overlays = [ (import-archive "mozilla/nixpkgs-mozilla" nixpkgs-mozilla) ];
};

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
    rust
    sccache
  ];
  shellHook = "export RUSTC_WRAPPER=sccache";
}
