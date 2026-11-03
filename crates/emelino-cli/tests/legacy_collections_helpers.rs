// Reused from fixed emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use legacy::{Options, Source, compile_sources};

fn output(original: &str) -> legacy::Output {
    let mut source = String::new();
    for line in original.split_inclusive('\n') {
        if let Some(header) = line.strip_prefix("pub fn ") {
            let name = header.split('(').next().unwrap();
            source.push_str(&format!("@export(c,\"{name}\")\n"));
        }
        source.push_str(line);
    }
    heap::with_heap_audit(
        compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(16384)
                .with_string_capacity(256),
        )
        .unwrap_or_else(|errors| panic!("{errors:?}")),
    )
}
const HEADERS: &str = r#"
#include <assert.h>
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}
"#;
