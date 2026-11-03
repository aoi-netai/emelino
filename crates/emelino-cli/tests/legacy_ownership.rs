// Reused from fixed emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use heap::with_heap_audit;

use legacy::{Options, Output, Source, compile_sources};
use support::compile_and_run;

fn compile_source(source: &str, options: &Options) -> Output {
    compile_sources(&[Source::new("main.emel", source)], "main.emel", options)
        .unwrap_or_else(|errors| panic!("P3 ownership source rejected: {errors:#?}"))
}

#[test]
fn finite_changing_recursive_types_and_deep_recursive_release_execute_from_source() {
    let output = with_heap_audit(compile_source(
        include_str!("compat/p3/recursive.emel"),
        &Options::default().with_heap_bytes(4 * 1024 * 1024),
    ));
    compile_and_run(&output, include_str!("compat/p3/recursive-harness.c"));
}
