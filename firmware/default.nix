let
  moz_overlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  nixpkgs = import <nixpkgs> { overlays = [ moz_overlay ]; };
in
  with nixpkgs;
  stdenv.mkDerivation {
    name = "poe";
    buildInputs = [
      ((rustChannelOf {
        date = "2018-04-16";
        channel = "nightly";
      }).rust.override {
        targets = [ "thumbv7m-none-eabi" ];
        extensions = [ "rust-std" "rustfmt-preview" ];
      })
      gcc-arm-embedded
    ];
  }
