with import <nixos> {};
runCommand "dummy" {
  buildInputs = [
    ((rustChannelOf{ date = "2018-02-19"; channel = "nightly"; }).rust.override{ extensions = [ "rust-src" ]; })
  ];
} ""
