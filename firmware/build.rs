use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());

    macro_rules! create_file {
        ($name:literal, $contents:expr) => {
            File::create(out.join($name))
                .expect(concat!("creating ", $name))
                .write_all($contents)
                .expect(concat!("writing ", $name))
        };
    }

    create_file!("memory.x", include_bytes!("memory.x"));
    println!("cargo:rustc-link-search={}", out.display());

    create_file!("not-found.http", include_bytes!("assets/404.txt"));
    create_file!("identify.http", include_bytes!("assets/identify-200.txt"));

    let html = deflate::deflate_bytes_gzip(include_bytes!("assets/index.html"));
    let mut http = String::from(include_str!("assets/index-200.txt"))
        .replace("{TIME}", &chrono::Utc::now().to_rfc2822())
        .replace("{SIZE}", &format!("{}", html.len()))
        .into_bytes();
    http.extend_from_slice(&html);
    create_file!("index.http", &http);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");
}
