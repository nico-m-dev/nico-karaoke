use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let web = manifest.join("../../web/dist");
    println!("cargo:rerun-if-changed={}", web.display());

    let mut files = Vec::new();
    if web.is_dir() {
        collect(&web, &web, &mut files);
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut code = String::from("pub static EMBEDDED_ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in files {
        println!("cargo:rerun-if-changed={}", path.display());
        code.push_str(&format!("    ({name:?}, include_bytes!({path:?})),\n"));
    }
    code.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("embedded_assets.rs");
    fs::write(out, code).expect("write embedded assets");
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            files.push((relative.to_string_lossy().replace('\\', "/"), path));
        }
    }
}
