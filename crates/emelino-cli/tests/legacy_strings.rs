// Reused from fixed emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use legacy::{Options, Source, compile_sources};

#[test]
fn original_string_comparisons_remain_heapless_without_scratch_and_allow_isr() {
    let source = include_str!(
        "compat/legacy-corpus/sources/36887fdd50ace6a21aeb4b08f49cc935bf137db1baeca65beecbe73f157e09e4.emel"
    );
    let exports = ["eq", "ne", "lt", "le", "gt", "ge", "repeated"].map(|name| (name, name));
    let output = support::compile(source, &exports, &Options::default().with_isr_entry("lt"));
    assert!(output.c_source.contains("emela_emb_string_compare"));
    assert!(!output.c_source.contains("emela_heap_storage"));
    assert!(!output.c_source.contains("char emela_string_buffer"));
}

#[test]
fn legacy_string_comparisons_preserve_captured_bytes_through_completion_and_cancel() {
    let original = include_str!(
        "compat/legacy-corpus/sources/e73ff5dc6bdbae6166012a370559dc06acda184140c57bffce000b364bb7beca.emel"
    );
    let source = original.replace(
        "pub fn across(",
        "@export(c,\"legacy_across\")\npub fn across(",
    );
    let mut output = heap::with_heap_audit(
        compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(4096)
                .with_fiber_capacity(2)
                .with_frame_capacity(8)
                .with_frame_bytes(512)
                .with_task_capacity(1)
                .with_event_capacity(1)
                .with_deferred_capacity(0)
                .with_task_adapter("host_wait", "board_wait_start"),
        )
        .unwrap_or_else(|errors| panic!("{errors:?}")),
    );
    assert!(output.c_source.contains("emela_emb_string_compare"));
    assert!(!output.c_source.contains("char emela_string_buffer"));
    let source = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    source.push_str("\nvoid test_cancel(EmelaFiber f){emela_fiber_get(f)->cancelled=1;}\n");
    output.c_source = source.clone();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber f);
static EmelaTask task;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t n){(void)n;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}
void board_wait_start(EmelaTask t,int32_t n){assert(n==1);task=t;}
static void drain(void){while(emela_scheduler_run(1)){}}
int main(void){for(unsigned i=0;i<1000;i++){for(unsigned route=0;route<3;route++){
 char a[]={'a',0,'b'},b[]={'a',0,'c'};int32_t result=-1;
 EmelaFiber f=legacy_across_start((EmelaString){a,3,NULL},(EmelaString){b,3,NULL},route==1?NULL:&result);
 memset(a,'z',3);memset(b,'z',3);drain();
 if(route==2){test_cancel(f);drain();}
 else{assert(emela_task_complete_int(task,1));drain();if(route==0)assert(result==1);}
 emela_fiber_release(f);assert(test_heap_empty());
}}return 0;}
"#,
    );
}
