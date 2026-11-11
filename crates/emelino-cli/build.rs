//! Package the firmware support tree with the executable, independent of checkout location.
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::{env, fs};

fn collect(root: &Path, path: &Path, files: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", path.display());
    assert!(
        !path.is_symlink(),
        "support inputs must not be symlinks: {}",
        path.display()
    );
    if path.is_dir() {
        for child in fs::read_dir(path).expect("read support directory") {
            let child = child.expect("read support entry").path();
            let name = child.file_name().unwrap().to_string_lossy();
            if !name.starts_with('.') && name != "__pycache__" {
                collect(root, &child, files);
            }
        }
    } else if !path.extension().is_some_and(|ext| ext == "pyc") {
        files.push(path.strip_prefix(root).unwrap().to_owned());
    }
}

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let mut files = Vec::new();
    for path in [
        "firmware",
        "tools/build_firmware.py",
        "tools/setup",
        "LICENSE",
    ] {
        collect(&root, &root.join(path), &mut files);
    }
    files.sort();
    let mut hash = DefaultHasher::new();
    let mut code = String::from("pub const FILES: &[(&str, &[u8])] = &[\n");
    for path in files {
        let bytes = fs::read(root.join(&path)).expect("read support file");
        path.hash(&mut hash);
        bytes.hash(&mut hash);
        code.push_str(&format!(
            "({:?}, include_bytes!({:?})),\n",
            path.to_str().unwrap(),
            root.join(&path).canonicalize().unwrap()
        ));
    }
    code.push_str(&format!(
        "];\npub const ID: &str = \"{:016x}\";\n",
        hash.finish()
    ));
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("support.rs"),
        code,
    )
    .expect("write bundled support table");
}
