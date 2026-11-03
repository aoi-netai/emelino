//! Original source/C observations with explicit, separately labelled IR test seams.
//! The fixed test code and C harnesses are reused from emela-emb b0a367bc.
use emelino_compiler::{Options, Output, ir::*};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "compat/seams/cause.rs"]
mod cause;
#[path = "compat/seams/cleanup.rs"]
mod cleanup;
#[path = "compat/seams/protect.rs"]
mod protect;

fn adapted_public(source: &str, prefix: &str) -> Program {
    let generated = emelino_cli::frontend::generate(source, Path::new("main.emel")).unwrap();
    let aliases: Vec<_> = generated
        .module
        .functions
        .iter()
        .filter(|(_, function)| function.exported)
        .map(|(_, function)| (function.name.clone(), format!("{prefix}{}", function.name)))
        .collect();
    emelino_cli::upstream_adapter::adapt(&generated.module, &aliases).unwrap()
}

fn binding(function: &mut Function, dst: Option<VarId>, value: Value) -> StmtId {
    function.stmts.alloc(Stmt::Let {
        dst,
        value,
        on_defect: None,
        on_suspend: None,
    })
}
fn audit_owned_heap(output: &mut Output) {
    let body = if output.c_source.contains("emela_heap_storage") {
        "int test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}"
    } else {
        "int test_heap_empty(void){return 1;}"
    };
    output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str(&format!("\n{body}\n"));
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained source seam fixture: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
fn execute(output: &Output, harness: &str, expected: i32) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = Scratch(std::env::temp_dir().join(format!(
        "emelino-source-seam-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(&temp.0).unwrap();
    for file in &output.files {
        std::fs::write(temp.0.join(&file.path), &file.contents).unwrap();
    }
    std::fs::write(temp.0.join("harness.c"), harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let result = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "emela_embedded.c",
                "harness.c",
                "-lm",
                "-o",
                "program",
            ])
            .current_dir(&temp.0)
            .output()
            .expect("C compiler required for source seam validation");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(temp.0.join("program")).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(expected),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
