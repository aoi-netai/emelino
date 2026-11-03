//! Fixed original const corpus through official Flow; optional explicit legacy GC seam.
//! Reused from emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
use super::*;

fn original(id: &str, exports: &[&str]) -> String {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/compat/legacy-corpus/manifest.json"
    ))
    .unwrap();
    let program = manifest["programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == format!("program:{id}"))
        .unwrap();
    assert_eq!(program["files"].as_array().unwrap().len(), 1);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compat/legacy-corpus")
        .join(program["files"][0]["fixture"].as_str().unwrap());
    let source = std::fs::read_to_string(path).unwrap();
    for name in exports {
        assert!(source.contains(&format!("pub fn {name}(")));
    }
    source
}

fn fixture(id: &str, exports: &[&str], options: &Options, gc: bool) -> Output {
    let source = original(id, exports);
    let aliases: Vec<_> = exports
        .iter()
        .map(|name| ((*name).to_owned(), (*name).to_owned()))
        .collect();
    let mut program = adapted(&source, &aliases);
    if gc {
        add_gc_anchor(&mut program);
    }
    emelino_compiler::compile_program(program, options)
        .unwrap_or_else(|errors| panic!("{id}: {errors:#?}"))
}

pub(super) fn add_gc_anchor(program: &mut Program) {
    // Same as the fixed old gc_anchor: enable tracing through reachable Cell[Any].
    let ty = program.core.types.intern(Type::Cell(TypeId::ANY));
    let mut anchor = simple();
    anchor.name = "test_gc_anchor".into();
    anchor.export_symbol = Some(anchor.name.clone());
    anchor.ret = TypeId::ANY;
    let value = aggregate_variable(&mut anchor, TypeId::ANY);
    anchor.params.push(value);
    let cell = aggregate_variable(&mut anchor, ty);
    let result = aggregate_variable(&mut anchor, TypeId::ANY);
    let make = binding(
        &mut anchor,
        Some(cell),
        Value::Builtin {
            op: Builtin::CellNew,
            args: vec![value.into()],
        },
    );
    let read = binding(
        &mut anchor,
        Some(result),
        Value::Builtin {
            op: Builtin::CellGet,
            args: vec![cell.into()],
        },
    );
    anchor.seqs[anchor.body.unwrap()] = Seq {
        stmts: vec![make, read],
        term: Term::Return(Some(result.into())),
    };
    program.functions.alloc(anchor);
}

const BOARD: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"%u: %.*s\n",k,n,m);abort();}
"#;

fn heap_probe(output: &mut Output, gc: bool, copy_string: bool) {
    let c = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    c.push_str("\nsize_t test_const_heap(void){");
    if gc {
        c.push_str("emela_heap_collect_cycles();");
    }
    c.push_str("size_t used=0;if(!emela_heap_initialized)return 0;for(unsigned char*p=emela_heap_storage;p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)used+=b->size;p+=b->size;}return used;}\n");
    if copy_string {
        c.push_str(
            "EmelaString test_const_owned(EmelaString v){return emela_heap_copy_string(v);}\n",
        );
    }
}

#[test]
fn original_const_roots_survive_cycle_collection() {
    let mut output = fixture(
        "661bd10d1eb4fcbe825b",
        &["read"],
        &Options::default().with_heap_bytes(4096),
        true,
    );
    heap_probe(&mut output, true, false);
    execute(
        &output,
        &format!(
            r#"{BOARD}
static int calls;void emela_board_debug(const char*s,uint16_t n){{(void)s;(void)n;calls++;}}
size_t test_const_heap(void);
int main(void){{assert(read()==42);size_t baseline=test_const_heap();assert(baseline);for(int i=0;i<1000;i++)assert(read()==42&&test_const_heap()==baseline);assert(calls==1);return 0;}}
"#
        ),
        0,
    );
}

#[test]
fn upstream_001_callable_families_keep_values_but_initialize_per_specialization() {
    // Original fixed-fork expected nine shared initializations. Pinned official
    // Flow emits twenty initialized const units; values and ownership still hold.
    let names = [
        "integer",
        "text",
        "evidence",
        "failing",
        "choice_integer",
        "choice_text",
        "enum_integer",
        "enum_text",
        "caught",
    ];
    for gc in [false, true] {
        let mut output = fixture(
            "3341457f78b9863e37dd",
            &names,
            &Options::default()
                .with_heap_bytes(32768)
                .with_frame_bytes(1024),
            gc,
        );
        heap_probe(&mut output, gc, false);
        execute(
            &output,
            &format!(
                r#"{BOARD}
static unsigned calls;void emela_board_debug(const char*s,uint16_t n){{(void)s;(void)n;calls++;}}
size_t test_const_heap(void);
int main(void){{size_t baseline=0;for(unsigned i=0;i<1001;i++){{assert(integer()==42&&text()==42&&evidence()==42&&failing(0)==42&&choice_integer()==42&&choice_text()==42&&enum_integer()==42&&enum_text()==42&&caught()==42);if(!i)baseline=test_const_heap();else assert(test_const_heap()==baseline);}}assert(calls==20);return 0;}}
"#
            ),
            0,
        );
    }
}

#[test]
fn original_const_sources_keep_recursive_closures_aggregates_and_discarded_first_reads() {
    for (id, expected, heap) in [
        ("12c836689dcf8fbdefcc", 42, 4096),
        ("fb872bf260d3593b4367", 44, 8192),
    ] {
        let output = fixture(
            id,
            &["read"],
            &Options::default().with_heap_bytes(heap),
            false,
        );
        execute(
            &output,
            &format!(
                r#"{BOARD}
static int calls;void emela_board_debug(const char*s,uint16_t n){{(void)s;(void)n;calls++;}}
int main(void){{for(int i=0;i<1000;i++)assert(read()=={expected});assert(calls==1);return 0;}}
"#
            ),
            0,
        );
    }
    let output = fixture(
        "356d9928e442971ded00",
        &["read", "discard"],
        &Options::default().with_heap_bytes(8192),
        false,
    );
    execute(
        &output,
        &format!(
            r#"{BOARD}
static int calls;int32_t fixture_seed(void){{return ++calls+41;}}
int main(void){{discard();for(int i=0;i<1000;i++)assert(read()==42);assert(calls==1);return 0;}}
"#
        ),
        0,
    );
}

#[test]
fn original_nested_const_defects_cache_the_exact_context_and_self_reference_retries() {
    let output = fixture(
        "fee491a366b1c1ad5051",
        &["bad", "self_ref"],
        &Options::default().with_heap_bytes(8192),
        false,
    );
    execute(
        &output,
        r#"
#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf target;static int calls,debug_calls,mode;
int32_t fixture_zero(void){calls++;return 0;}
void emela_board_debug(const char*s,uint16_t n){(void)s;(void)n;debug_calls++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){const char*expected=mode?"const SELF refers to itself while being evaluated":"while evaluating const A: while evaluating const B: division by zero";assert(n==strlen(expected)&&!memcmp(m,expected,n));if(!mode)assert(k==EMELA_DEFECT_DIVISION_BY_ZERO);longjmp(target,1);}
int main(void){for(volatile int i=0;i<1000;i++)if(setjmp(target)==0){bad();abort();}assert(calls==1&&debug_calls==1);mode=1;for(volatile int i=0;i<1000;i++)if(setjmp(target)==0){self_ref();abort();}assert(calls==1001);return 0;}
"#,
        0,
    );
}

#[test]
fn original_exponential_static_string_uses_bounded_runtime_evaluation() {
    let output = fixture(
        "2f21eb2017b3f0b1d760",
        &["size"],
        &Options::default()
            .with_heap_bytes(1024)
            .with_string_capacity(32),
        false,
    );
    execute(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){static const char prefix[]="while evaluating const VALUE: ";exit(k==EMELA_DEFECT_STRING_CAPACITY&&n>=sizeof prefix-1&&!memcmp(m,prefix,sizeof prefix-1)?0:2);}
int main(void){size();return 1;}
"#,
        0,
    );
}
