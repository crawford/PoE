with import <nixos> {};
runCommand "dummy" {
  buildInputs = [
    ((rustChannelOf{ date = "2018-01-16"; channel = "nightly"; }).rust.override{ extensions = [ "rust-src" ]; })
  ];
} ""
