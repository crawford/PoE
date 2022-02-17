use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .expect("creating memory.x")
        .write_all(include_bytes!("memory.x"))
        .expect("writing memory.x");
    println!("cargo:rustc-link-search={}", out.display());

    let html = deflate::deflate_bytes_gzip(include_bytes!("assets/index.html"));
    File::create(out.join("index-200.txt"))
        .expect("creating index-200.txt")
        .write_all(
            String::from(include_str!("assets/index-200.txt"))
                .replace("{TIME}", &chrono::Local::now().to_rfc2822())
                .replace("{SIZE}", &format!("{}", html.len()))
                .as_bytes(),
        )
        .expect("writing index-200.txt");
    File::create(out.join("index.html"))
        .expect("creating index.html")
        .write_all(&html)
        .expect("writing index.html");
    File::create(out.join("value-200.txt"))
        .expect("creating value-200.txt")
        .write_all(include_bytes!("assets/value-200.txt"))
        .expect("writing value-200.txt");
    File::create(out.join("400.txt"))
        .expect("creating 400.txt")
        .write_all(include_bytes!("assets/400.txt"))
        .expect("writing 400.txt");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");
}
