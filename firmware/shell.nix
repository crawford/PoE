with import <nixpkgs> {
  overlays = [ (import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz)) ];
};

stdenv.mkDerivation {
  name = "poe";
  buildInputs = [
    ((rustChannelOf {
      date = "2018-06-09";
      channel = "nightly";
    }).rust.override {
      targets = [ "thumbv7m-none-eabi" ];
      extensions = [ "rust-std" "rustfmt-preview" ];
    })
    gcc-arm-embedded
  ];
  shellHook = "export CC_thumbv7m_none_eabi=arm-none-eabi-gcc";
}
