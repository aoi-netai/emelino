// Reused from fixed emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use legacy::{Options, Output, Source, compile_sources};

fn suspended(original: &str, name: &str, options: &Options) -> Output {
    let source = original.replace(
        &format!("pub fn {name}("),
        &format!("@export(c,\"legacy_{name}\")\npub fn {name}("),
    );
    let mut output = heap::with_heap_audit(
        compile_sources(&[Source::new("main.emel", &source)], "main.emel", options)
            .unwrap_or_else(|errors| panic!("{errors:?}")),
    );
    let source = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    // The old test directly requests root cancellation through the private runtime.
    source.push_str("\nvoid test_cancel(EmelaFiber f){emela_fiber_get(f)->cancelled=1;}\n");
    output.c_source = source.clone();
    output
}

const HEADERS: &str = r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber f);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}
"#;
