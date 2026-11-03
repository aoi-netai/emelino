// Reused from fixed emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use legacy::{Options, Output, Source, compile_sources};

fn fixture() -> Output {
    let original = include_str!(
        "compat/legacy-corpus/sources/c2cacb8eb0a458543a2c75f44e74015be514f14c24db2ca938af34204d514b74.emel"
    );
    let mut source = original.to_owned();
    for name in [
        "traits",
        "quoted",
        "debugging",
        "written_cause",
        "derived_cause",
        "plain_cause",
        "entry_failure",
    ] {
        source = source.replace(
            &format!("pub fn {name}("),
            &format!("@export(c,\"{name}\")\npub fn {name}("),
        );
    }
    heap::with_heap_audit(
        compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(16384)
                .with_string_capacity(256),
        )
        .unwrap_or_else(|errors| panic!("legacy traits: {errors:#?}")),
    )
}

const BASE: &str = r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static void equal_text(EmelaString text,const char*expected){size_t n=strlen(expected);if(text.length!=n||memcmp(text.data,expected,n)){fprintf(stderr,"got %.*s (%u), expected %s (%zu)\n",text.length,text.data,text.length,expected,n);abort();}}
"#;
